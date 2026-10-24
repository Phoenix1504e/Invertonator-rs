// Invertonator-rs — tracker: media classification gates → live holes
//
//   Runs a discovery loop on its own thread: it walks the FOREGROUND
//   window's UIA tree (cached walk) and turns media elements into
//   HoleSpecs for the hole pump. Cadence is ADAPTIVE: 400ms while the
//   page is changing, backing off to 1500ms when the hole set is
//   stable — every walk makes the provider (Chromium) serialize its
//   accessibility tree, and a fixed 400ms tax on a playing video reads
//   as in-player stutter.
//
//   CLASSIFICATION GATES (the "what deserves a hole" rules):
//   · Document-pane scoping — media is only collected INSIDE
//     UIA_DocumentControlType subtrees. Browser chrome (tab strip,
//     address bar, bookmarks) lives OUTSIDE the document pane, so
//     this one rule IS browser-chrome preservation.
//   · Numeric control types, NOT localized strings — UIA_Image (50006)
//     and UIA_Hyperlink (50005) are locale-proof; matching "Bild" on a
//     German Windows would never work with string gates.
//   · "video" and "graphic" have no standard numeric type — Chromium
//     reports them via LocalizedControlType, matched ASCII-case-
//     insensitively.
//   · Size gates 64×64 px for image/graphic AND video (alpha.6: was
//     40/48 — every 40–63 px UI icon then punched a "small square
//     scattered on UI elements"), hyperlink ≥ 96×96 px (thumbnail-
//     sized links like the YouTube grid; text links stay inverted).
//     Real media is ≥64 px on both axes in practice; the sidebar
//     thumbnails (~168×94) still clear the bar via their Image child.
//   · Depth 60 (alpha.4 capped at 25): YouTube's DOM runs deeper than
//     25 levels — sidebar thumbnails and watch-page media sit BELOW
//     the old cap and were silently pruned.
//   · WARM-UP (alpha.6 — the "media outside the player" fix): the
//     probe's logs showed Chromium hands a UIA client a PARTIALLY
//     materialized tree on first contact (probe: 419 nodes; the warm-
//     up FindAll then expanded it). The tracker walked that starved
//     tree — concentrated around the player — so sidebar/watch media
//     below the fold was never seen at all. The tracker now runs the
//     same warm-up FindAll ONCE per foreground window, and if a cached
//     walk still comes back starved (< warm-up/2 nodes), it does a
//     full live FindAll walk that tick (2s cooldown).
//   · Clamp to the foreground window + 4px quantize: off-screen
//     garbage rects never become holes, and UIA's ±1-3px rect wobble
//     no longer moves holes / re-arms captures every walk.
//   · Merge (alpha.9 — SAME-NAME clustering): transitive union-find
//     clustering over near() pairs; the gap tolerance keys on the
//     accessible NAMES:
//     · both candidates UNNAMED (empty accessible name — player
//       canvas/poster/tile layers): tolerance = max(8px, 6.25% of
//       the smaller fragment's larger axis) — seams up to ~45px on
//       a 720p media merge into the ONE big hole the user expects.
//     · both named AND THE SAME NAME (alpha.9): same scaled
//       tolerance. alpha.8 sent every named pair to the 8px floor
//       and the player fragmented AGAIN — its fragments evidently
//       CARRY names (the tester's own dump showed the hole split as
//       soon as the floor landed). Fragments of one rendered media
//       share its title; distinct titles are distinct page items.
//     · any other pairing (one named, or two DIFFERENT names):
//       the 8px floor. A differently-named candidate can never
//       chain across a real page gutter — this keeps the alpha.7
//       mega-hole dead: grid thumbnails carry distinct video
//       titles, so no transitive chain can cross a gutter.
//   · Walk-tax discipline (alpha.9, tightened in alpha.10): every
//     walk makes the provider (Chromium) serialize its a11y tree, and
//     that tax competes with the video renderer. While a DYNAMIC hole
//     is live the cadence floor is 800ms; the stable backoff cap is
//     1500ms — alpha.9's 3000ms cap let holes go STALE for seconds
//     after every scroll (the "too many rectangles and on random
//     places" regression). Freshness is enforced by the WAKE LOOP
//     (alpha.10): the discovery nap is sliced into 50ms steps and cut
//     short the moment a scroll burst ends, a wheel fires, the
//     foreground window moves/resizes/changes, or its TITLE changes
//     (alpha.11 — a tab switch changes the title and fires NO wheel
//     event, so the old tab's holes used to linger; and if the walk
//     then wedged in the tearing-down provider they lingered FOREVER
//     — the "holes are permanent even after shifting tabs" report).
//     The next walk happens ~50ms after any of these, whatever the
//     backoff was. The starvation live-walk cooldown stays widened
//     2s → 5s (alpha.9).
//   · EVIDENCE (alpha.11 — see holes.rs STALE_SPEC_AGE): the pump
//     hides every hole not re-confirmed by a completed walk within
//     8s, so a wedged/panicked tracker degrades to "no holes" and
//     recovers automatically instead of freezing stale holes on
//     screen forever. Walks slower than 1s are logged as warnings —
//     the wedged-provider signature, visible in log.txt without any
//     user diagnosis.
//   · SMOOTHNESS (alpha.12 — the "video is a little slow in the
//     hole" fix, holes.rs side): pump 15ms + timeBeginPeriod(1) —
//     the old 30ms pump actually fired at 22–32Hz on the 15.6ms
//     scheduler grid — and UNLAYERED hosts (an opaque layered host
//     pushed every repaint through DWM redirection composition). The
//     pump heartbeat now logs the ACTUAL delivery Hz every 10s.
//   · PRECISION (alpha.12 — the "random holes still appear" fix):
//     two garbage-rect generators removed. (1) OFFSCREEN FILTER:
//     IsOffscreen=true elements (display:none, scrolled-out,
//     virtualized placeholders) never become candidates — their rects
//     are stale or huge, and clamping a huge rect into the fg window
//     pins a hole to a random place. (2) STARVED PUBLISH GUARD: a
//     cached walk that sees less than half the warm-up population is
//     a half-built tree — its placeholder rects are no longer
//     published; holes hide until a trustworthy walk completes. The
//     warm-up now re-runs when the fg TITLE changes (tab switch /
//     navigation), so the starvation baseline describes the CURRENT
//     page (a smaller next tab used to read as chronic starvation),
//     and every live walk re-baselines it from the real population.
//   · HOLE AVAILABILITY (alpha.13 — "the holes on youtube are gone
//     now"): the alpha.12 starved-suppression over-fired. On a heavy
//     page (YouTube watch) the CHEAP cached walk chronically returns
//     a half-materialized tree — Chromium's CachedChildren view never
//     approaches the warm-up FindAll population — so suppression hit
//     nearly every tick while the only walk that actually FINDS the
//     video (the live FindAll) waited out its 5s cooldown: holes
//     flashed for <1s and vanished for ~4s in a loop, i.e. "gone".
//     VSCodium-class pages are never starved and were perfect. Two
//     changes, both invisible on non-starved pages:
//     (1) STICKY TRUST — a starved tick no longer publishes empty;
//     it re-publishes the last TRUSTED hole set (the latest live-walk
//     or full-cached result), so holes stay on screen between live
//     walks instead of blinking out. Trusted sets replace sticky on
//     every healthy walk — including healthy empties (a real no-media
//     page clears the holes as before).
//     (2) EXPEDITED LIVE — a wake walk (scroll end, wheel, fg
//     move/resize/change, title change, F5-ON) that comes back
//     starved re-walks after the tree settles and, if STILL starved,
//     runs the live FindAll IMMEDIATELY. The 5s cooldown caps the
//     IDLE live rate (anti-stutter); user-initiated freshness is rare
//     by construction and must not queue behind it. Tab switches and
//     alt-tabs settle SETTLE_MS first — the same teardown window the
//     alpha.11 title wake used to walk straight into.
//   · FAIL-OPEN OFFSCREEN (alpha.14 — "no hole in youtube", round 2):
//     the alpha.12 IsOffscreen gate hard-suppressed elements on BOTH
//     walk paths, and the alpha.13 canary covered only video-kind —
//     but the tester's own dumps show the YouTube hole is built from
//     IMAGE-kind poster/player layers (img*3[YouTu…]). Chromium flags
//     exactly those offscreen on some pages, so every walk returned
//     ZERO candidates and the holes were gone everywhere, forever,
//     while VSCodium (never flagged, never starved) stayed perfect.
//     The flag is now a HINT: an offscreen-flagged element is still
//     offered when its geometry is independently usable (see
//     offscreen_trust) — the fg clamp, 64px size gates, giant filter
//     and merge all re-judge the rect on its own merits. Admissions
//     are counted (OFFSCREEN_ADMITS, in the diagnostics report) and
//     the first one logs its kind + rect.
//   · CONTEXT-AWARE EVIDENCE (alpha.14 — holes.rs STALE_SPEC_AGE):
//     the alpha.11 TTL hid holes 8s after the last completed walk,
//     whatever the reason — a chronically slow provider (YouTube under
//     load) hid holes that were still CORRECT. The pump now hides on
//     staleness only when the foreground CONTEXT changed since the
//     publish (fg identity = hwnd + fnv1a(title), tracker::fg_identity).
//     Same context = the screen still shows what the specs describe —
//     slow walks can no longer blank an unchanged page. Changed
//     context (tab switch / alt-tab / retitle) hides unconfirmed holes
//     within 4s — the alpha.10 guarantee, now context-scoped and twice
//     as fast as the old blanket TTL.
//   · Giant filter: a NON-dynamic candidate covering ≥ 85% of the
//     foreground window is clamped garbage or a page wrapper — it
//     would absorb the entire clustering. Dropped.
//   · Final order is deterministic (top-to-bottom, left-to-right),
//     so an unchanged hole set keeps its pump index mapping — no
//     hole-window churn when two specs merely swap slots.
//   · Cap at MAX_HOLES, largest first.
//
//   COORDINATES: screen physical pixels end to end (per-monitor-v2
//   aware — see main::set_dpi_awareness).

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    UIA_BoundingRectanglePropertyId, UIA_ControlTypePropertyId, UIA_DocumentControlTypeId,
    UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId, UIA_IsOffscreenPropertyId,
    UIA_LocalizedControlTypePropertyId, UIA_NamePropertyId, UIA_CONTROLTYPE_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowTextW};

use crate::holes::{HoleSpec, Holes, MAX_HOLES};

const DISCOVERY_MS: u64 = 400; // active cadence (page changing)
const MAX_DISCOVERY_MS: u64 = 1500; // idle backoff cap (page stable)
                                    // alpha.9 — while a DYNAMIC hole is live (video playing) the walk tax
                                    // lands on the browser's renderer, so discovery breathes. alpha.10 —
                                    // the CAP stays at 1500ms: the pump re-shows holes ~350ms after a
                                    // scroll ends with whatever specs are published, so a longer backoff
                                    // means visible holes at STALE coordinates (the alpha.9 "too many
                                    // rectangles and on random places" regression). Moment-to-moment
                                    // freshness is the WAKE LOOP's job; the backoff only saves tax while
                                    // nothing changes.
const DISCOVERY_PLAYING_MS: u64 = 800;
const MAX_DISCOVERY_PLAYING_MS: u64 = 1500;
const MIN_MEDIA: i32 = 64; // image/graphic AND video min w and h (px)
const MIN_LINK: i32 = 96; // hyperlink min w and h (px)
const MIN_HOLE: i32 = 32; // final hole min — degenerate slivers out
const MERGE_GAP: i32 = 8; // gap-tolerance floor (named media + small media)
                          // Unnamed pairs (fragments of one media) AND same-name
                          // pairs scale above the floor with the SMALLER fragment's
                          // larger axis (÷16 ≈ 6.25%). Distinct-named pairs never
                          // leave the floor — see the merge notes in the header.
const GIANT_SHARE: f64 = 0.85; // non-dynamic candidate vs fg window area
const QUANTUM: i32 = 4; // rect quantization grid (px)
const MAX_NODES: u32 = 100_000;
const MAX_DEPTH: u32 = 60; // alpha.4 capped at 25 — YouTube runs deeper
const LIVE_COOLDOWN_MS: u64 = 5000; // starvation live-walk throttle
                                    // (alpha.9: was 2s — the live FindAll is the
                                    // HEAVIEST walk; every 2s it stuttered the video)
const SLEEP_SLICE_MS: u64 = 50; // wake-loop nap slice (alpha.10)
const SETTLE_MS: u64 = 600; // alpha.13 — post-teardown grace before a
                            // starved wake walk re-walks (tab switch /
                            // alt-tab): lets the provider finish
                            // destroying/rebuilding its tree, so the
                            // re-walk sees the NEW page instead of the
                            // tearing-down one (also the alpha.11 wedge
                            // window — walking it at +50ms was the risk)

/// One-shot flag: the Gecko live-fallback notice is logged exactly once
/// per process (discovery ticks every 400ms — per-tick logs would spam
/// log.txt at 150 lines/minute).
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

/// alpha.12 — starvation-suppression transition logger (see discover).
static STARVED_QUIET: AtomicBool = AtomicBool::new(false);

/// alpha.13 diagnostics counters, read by the probe report
/// (win_main::probe_report). STARVED_TICKS = suppressed starved ticks
/// since process start — a big number next to a healthy walk count is
/// the "cached walk is chronically half-tree on this page" signature
/// (the YouTube failure mode). STICKY_HELD = hole specs currently held
/// by the sticky-trust bridge.
pub static STARVED_TICKS: AtomicU64 = AtomicU64::new(0);
pub static STICKY_HELD: AtomicUsize = AtomicUsize::new(0);

/// alpha.14 fail-open telemetry: OFFSCREEN_ADMITS counts elements
/// offered despite IsOffscreen=true (the fail-open path — see
/// offscreen_trust); OFFSCREEN_ADMIT_LOGGED rate-limits the detail
/// line to once per process. A climbing count while holes are missing
/// elsewhere is the "Chromium flags visible media offscreen"
/// fingerprint; a count of ZERO next to missing holes rules this path
/// out. (The alpha.13 video-only canary could not see image-kind
/// suppressions — and image layers are what the YouTube hole is built
/// from — which is exactly why the gate went fail-open instead.)
pub static OFFSCREEN_ADMITS: AtomicU64 = AtomicU64::new(0);
static OFFSCREEN_ADMIT_LOGGED: AtomicBool = AtomicBool::new(false);

/// alpha.10 — wake signal: the discovery nap ends early whenever this
/// is set. Raised by set_enabled(true) so F5-ON produces holes in ~50ms
/// instead of after one full interval sleep.
static WAKE: AtomicBool = AtomicBool::new(false);

// =====================================================================
// Public API
// =====================================================================

pub struct Tracker {
    holes: Holes,
    enabled: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Tracker {
    /// Starts the discovery thread. `holes` is a shared-state handle —
    /// the thread only ever calls set_specs on it.
    pub fn spawn(holes: Holes) -> Self {
        let enabled = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));

        let th_enabled = enabled.clone();
        let th_stop = stop.clone();
        let th_holes = holes.clone();

        let thread = std::thread::Builder::new()
            .name("media-tracker".into())
            .spawn(move || discovery_loop(th_holes, th_enabled, th_stop))
            .expect("tracker: discovery thread spawn");

        Self {
            holes,
            enabled,
            stop,
            thread: Some(thread),
        }
    }

    /// true = discover holes for the foreground window. false = clear
    /// and idle. Called from apply_mode (holes exist in Invert only).
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if on {
            // alpha.10: don't wait out the current nap — walk now, so
            // F5-ON puts holes on screen in ~50ms.
            WAKE.store(true, Ordering::Relaxed);
        } else {
            // Immediate clear — don't wait for the discovery tick.
            self.holes.set_specs(Vec::new());
        }
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// =====================================================================
// Discovery loop
// =====================================================================

fn discovery_loop(holes: Holes, enabled: Arc<AtomicBool>, stop: Arc<AtomicBool>) {
    // COM on THIS thread, once. UIA client objects are used from the
    // thread that created them.
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            tracing::info!("tracker: CoInitializeEx: {hr} (ok if already init)");
        }
    }
    let automation: IUIAutomation =
        match unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) } {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("tracker: CoCreateInstance failed: {e} — tracker dead");
                return;
            }
        };
    tracing::info!("tracker: alive (discovery every {DISCOVERY_MS}ms)");

    let mut had_specs = false;
    let mut last_sig = String::new();
    let mut last_rects = String::new();
    let mut last_cands = String::new();
    let mut last_fg: usize = 0;
    let mut interval = DISCOVERY_MS;
    let mut last_beat = std::time::Instant::now();
    // alpha.10 wake-loop state: the fg window rect as of the last walk
    // (a moved/resized fg window invalidates every hole — they are
    // absolute-screen rects) and the previous hold_active() sample
    // (a scroll burst ENDING is the moment holes reappear and MUST be
    // re-synced immediately).
    let mut last_fg_rect = RECT::default();
    let mut was_hold = false;
    // alpha.11 tab-switch wake: the fg window's title as of the last
    // walk. A tab switch changes it and fires NO wheel event and NO
    // fg-rect change — without this signal the walk kept the old tab's
    // holes until the backoff expired (and forever, when the next walk
    // wedged inside the tearing-down provider).
    let mut last_title = String::new();
    // Warm-up + live-walk state (alpha.6): the warm-up runs once per
    // (window, TITLE) pair since alpha.12 — a tab switch or navigation
    // changes the content behind the same HWND, and the starvation
    // baseline must describe the CURRENT page. The starvation
    // live-walk is throttled.
    let mut warm_key_last = String::new();
    let mut warm_total: u32 = 0;
    let mut last_live = std::time::Instant::now();
    // alpha.13 sticky trust: the last TRUSTED hole set (live-walk or
    // full-cached result). A starved tick re-publishes this instead of
    // empty — holes must not vanish just because Chromium's cached
    // tree is half-materialized between live walks.
    let mut sticky_specs: Vec<HoleSpec> = Vec::new();
    // consecutive suppressed starved ticks (heartbeat telemetry)
    let mut starved_run: u32 = 0;
    // alpha.13 wake reasons, raised by the nap loop, consumed by the
    // next walk. pending_wake alone = scroll/rect/F5 wake (expedite
    // only); the title/fg variants additionally settle first — those
    // are the teardown moments.
    let mut pending_wake = false;
    let mut pending_wake_title = false;
    let mut pending_wake_fg = false;
    while !stop.load(Ordering::Relaxed) {
        // Consume wake reasons left over from the nap (and a WAKE raised
        // while the previous walk ran — e.g. F5 pressed mid-walk).
        let wake_walk = pending_wake || WAKE.swap(false, Ordering::Relaxed);
        let wake_title = pending_wake_title;
        let wake_fg = pending_wake_fg;
        pending_wake = false;
        pending_wake_title = false;
        pending_wake_fg = false;
        if enabled.load(Ordering::Relaxed) {
            let fg = unsafe { GetForegroundWindow() };
            let t0 = std::time::Instant::now();
            let title = window_title(fg);
            let (specs, cand_dump) = if fg.is_invalid() {
                (Vec::new(), String::new())
            } else {
                // WARM-UP (alpha.6): first contact with a foreground
                // window — force the provider to materialize its full
                // accessibility tree and record how big it really is.
                // Chromium hands out a PARTIAL tree otherwise, and the
                // tracker then only ever saw the player-adjacent part
                // ("media outside the player is not recognised").
                // alpha.12: keyed on (HWND, TITLE) — a tab switch or
                // navigation keeps the HWND but swaps the content, and
                // a smaller next page must not read as "half a tree".
                let warm_key = format!("{:x}|{title}", fg.0 as usize);
                if warm_key != warm_key_last {
                    warm_key_last = warm_key;
                    let (n, rect_line) = unsafe { warmup(&automation, fg) };
                    warm_total = n;
                    if !rect_line.is_empty() {
                        tracing::info!("tracker: {rect_line}");
                    }
                    if warm_total > 0 {
                        tracing::info!("tracker: warm-up FindAll: {warm_total} elements");
                    }
                }
                let allow_live =
                    last_live.elapsed() >= std::time::Duration::from_millis(LIVE_COOLDOWN_MS);
                // alpha.13: a wake walk (scroll end, wheel, fg
                // move/resize/change, title change, F5-ON) may expedite
                // the live walk when the cached tree comes back starved;
                // title changes and fg switches settle first — those are
                // the provider-teardown moments.
                let expedite = wake_walk || wake_title || wake_fg;
                let settle = wake_title || wake_fg;
                let (s, dump, went_live, starved) = unsafe {
                    discover(
                        &automation,
                        fg,
                        &mut warm_total,
                        allow_live,
                        expedite,
                        settle,
                    )
                };
                if went_live {
                    last_live = std::time::Instant::now();
                }
                if starved {
                    // STICKY TRUST (alpha.13): keep the last trusted set
                    // on screen — an empty publish here is what made the
                    // holes "go gone" on YouTube between live walks.
                    starved_run = starved_run.saturating_add(1);
                    STARVED_TICKS.fetch_add(1, Ordering::Relaxed);
                    (sticky_specs.clone(), String::new())
                } else {
                    starved_run = 0;
                    sticky_specs = s.clone();
                    STICKY_HELD.store(sticky_specs.len(), Ordering::Relaxed);
                    (s, dump)
                }
            };
            let walk_ms = t0.elapsed().as_millis();
            // SLOW-WALK SIGNATURE (alpha.11): a completed-but-slow walk
            // means the provider was busy or half-torn-down (tab switch,
            // heavy page). One warn line per offense — this is what the
            // "holes froze/wrong for a while" reports look like in the
            // log, no user diagnosis needed.
            if walk_ms > 1000 {
                tracing::warn!(
                    "tracker: SLOW walk {walk_ms}ms — provider busy or wedged (tab teardown?)"
                );
            }
            let n_holes = specs.len();

            // CANDIDATE OBSERVABILITY (alpha.6): every gated candidate
            // (BEFORE merging), so the log shows exactly which elements
            // the walker saw and where — the ground truth for any
            // "hole is in the wrong place" report.
            if cand_dump != last_cands {
                if !cand_dump.is_empty() {
                    tracing::info!("tracker: cands [{cand_dump}]");
                }
                last_cands = cand_dump;
            }

            // HOLE-SET OBSERVABILITY (alpha.5): whenever the hole set
            // changes shape, dump every hole rect. This is the ground
            // truth for any "hole is in the wrong place" report — no
            // more guessing what the walker actually produced.
            let rects = specs
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    format!(
                        "#{i} {}x{} @({},{}){}",
                        s.w,
                        s.h,
                        s.x,
                        s.y,
                        if s.dynamic { " dyn" } else { "" }
                    )
                })
                .collect::<Vec<_>>()
                .join("  ");
            if rects != last_rects {
                tracing::info!("tracker: holes={n_holes} [{rects}]");
            }

            had_specs = !specs.is_empty();
            // alpha.9: video-aware cadence — decide BEFORE handing the
            // specs to the pump.
            let has_dyn = specs.iter().any(|s| s.dynamic);
            // alpha.14 — publish WITH the context it describes (fg hwnd
            // + page-title hash): the pump's evidence rule keys on it
            // (see holes.rs — CONTEXT-AWARE EVIDENCE). `title` is the
            // string this walk actually saw — the honest stamp for
            // these specs, even if the user alt-tabs DURING a long walk.
            let fg_ctx = (fg.0 as usize, fnv1a(&title));
            holes.set_specs_ctx(specs, fg_ctx);

            // ADAPTIVE CADENCE (alpha.9 — video-aware): a stable page
            // (same fg window, same hole set) backs the walk off in
            // doubling steps; ANY change snaps it back to the base.
            // The base/cap depend on whether a video hole is live:
            // its tax competes with the renderer, so while one plays
            // discovery breathes at 1s/3s instead of 400ms/1.5s.
            let (base, cap) = if has_dyn {
                (DISCOVERY_PLAYING_MS, MAX_DISCOVERY_PLAYING_MS)
            } else {
                (DISCOVERY_MS, MAX_DISCOVERY_MS)
            };
            let stable = rects == last_rects && fg.0 as usize == last_fg;
            if stable {
                interval = (interval * 2).min(cap);
            } else {
                interval = base;
            }
            last_fg = fg.0 as usize;
            last_rects = rects;
            let mut fr = RECT::default();
            if unsafe { GetWindowRect(fg, &mut fr) }.is_ok() {
                last_fg_rect = fr;
            }

            // OBSERVABILITY (alpha.4): log on foreground change, on
            // holes appearing/vanishing, or every 10s as a heartbeat.
            // If this line goes silent for >10s while inversion is on,
            // the walk is stuck inside a hung provider — that is the
            // "holes vanish and never come back" signature.
            // last_title tracks EVERY walk (the wake loop compares it);
            // the log line is conditional — so move `title` into
            // last_title after the log.
            let sig = format!("{}:{had_specs}", fg.0 as usize);
            if sig != last_sig || last_beat.elapsed() >= std::time::Duration::from_secs(10) {
                // starved= consecutive suppressed ticks — a page whose
                // cached walk is chronically half-tree shows this climbing
                // in every heartbeat (the alpha.13 YouTube signature).
                tracing::info!(
                    "tracker: fg=\"{title}\" holes={n_holes} walk={walk_ms}ms cadence={interval}ms starved={starved_run}"
                );
                last_sig = sig;
                last_beat = std::time::Instant::now();
            }
            last_title = title;
        } else if had_specs {
            // Disabled while holes were out — clear once.
            holes.set_specs(Vec::new());
            had_specs = false;
            // alpha.13: nothing is trusted across an F5-OFF/ON cycle.
            sticky_specs.clear();
            STICKY_HELD.store(0, Ordering::Relaxed);
        }
        // WAKE LOOP (alpha.10 — the "random places" fix): nap in 50ms
        // slices instead of one long sleep, and cut the nap short the
        // moment the published hole set may have gone stale:
        //   · a scroll burst ENDED (hold true→false) — the pump is
        //     about to re-show holes that still sit at pre-scroll
        //     coordinates; walking now re-syncs them within ~50ms;
        //   · a wheel fired with no hold live (belt-and-suspenders —
        //     the scroll hook sets the dirty flag alongside the hold);
        //   · the foreground window moved / resized / changed — holes
        //     are absolute-screen rects, a moved window invalidates
        //     every one of them;
        //   · the fg window's TITLE changed (alpha.11) — a tab switch
        //     (Ctrl+Tab, tab click) fires NO wheel event and keeps the
        //     same HWND and rect; the title is the one cheap signal the
        //     tracker can see. Without it the old tab's holes lingered
        //     until the backoff expired — and if the walk into the
        //     tearing-down provider then wedged, FOREVER (the pump now
        //     also self-defends with the holes.rs evidence TTL).
        //   · set_enabled(true) raised WAKE — F5-ON shows holes fast.
        // alpha.9 slept straight through all of this (backoff up to
        // 3s), so after each scroll the pump re-showed STALE holes —
        // "too many rectangles and on random places". While a burst is
        // LIVE the checks stay quiet: holes are hidden anyway and a
        // walk mid-flick is pure browser tax.
        let mut remaining = interval;
        while remaining > 0 && !stop.load(Ordering::Relaxed) {
            let enabled_now = enabled.load(Ordering::Relaxed);
            let mut wake = WAKE.swap(false, Ordering::Relaxed);
            if wake {
                // F5-ON mid-nap — a user-initiated freshness walk.
                pending_wake = true;
            }
            if enabled_now && !wake {
                let holding = crate::scrollhook::hold_active();
                if holding {
                    // Mid-burst: swallow the dirty flag, wait for the
                    // burst-end transition to do the walk.
                    was_hold = true;
                    let _ = crate::scrollhook::take_page_dirty();
                } else {
                    if was_hold || crate::scrollhook::take_page_dirty() {
                        wake = true;
                        // Scroll burst just ended / wheel fired — the
                        // page content moved; expedite the re-walk.
                        pending_wake = true;
                    }
                    was_hold = false;
                }
                if !wake {
                    let w = unsafe { GetForegroundWindow() };
                    let wu = w.0 as usize;
                    if last_fg != 0 && wu != last_fg {
                        wake = true;
                        // New foreground window — fresh provider, may
                        // still be building; expedite AND settle.
                        pending_wake = true;
                        pending_wake_fg = true;
                    } else if wu == last_fg && !w.is_invalid() {
                        let mut r = RECT::default();
                        if unsafe { GetWindowRect(w, &mut r) }.is_ok()
                            && (r.left != last_fg_rect.left
                                || r.top != last_fg_rect.top
                                || r.right != last_fg_rect.right
                                || r.bottom != last_fg_rect.bottom)
                        {
                            wake = true;
                            // Window moved/resized — rects are stale,
                            // but the provider is not tearing down:
                            // expedite without settle.
                            pending_wake = true;
                        }
                        // TAB SWITCH (alpha.11): same window, same rect,
                        // no wheel — the title is the only cheap signal
                        // that the page content just changed wholesale.
                        // Cross-process GetWindowTextW reads the cached
                        // caption (no message sent); cheap enough for a
                        // 50ms-slice poll.
                        if !wake && !last_title.is_empty() && window_title(w) != last_title {
                            wake = true;
                            // alpha.13: a title change IS a teardown /
                            // rebuild moment — expedite AND settle.
                            pending_wake = true;
                            pending_wake_title = true;
                        }
                    }
                }
            }
            if wake {
                break;
            }
            let step = remaining.min(SLEEP_SLICE_MS);
            std::thread::sleep(Duration::from_millis(step));
            remaining -= step;
        }
    }
    tracing::info!("tracker: stopped");
}

/// Locale-safe foreground title for the observability line.
fn window_title(hwnd: HWND) -> String {
    unsafe {
        let mut buf = [0u16; 128];
        let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
        let s = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        s.chars().take(40).collect()
    }
}

/// alpha.14 — CONTEXT identity of a window: (hwnd, fnv1a(title)). The
/// cheap, UIA-free fingerprint of "which window + which page" a spec
/// set describes. The tracker stamps it into every publish; the pump
/// samples the CURRENT context at ~2Hz and keeps holes on screen while
/// the two match (see holes.rs — CONTEXT-AWARE EVIDENCE). The title is
/// the same 40-char truncation the wake loop compares, so both sides
/// hash identical strings.
pub fn fg_identity(hwnd: HWND) -> (usize, u64) {
    if hwnd.is_invalid() || hwnd.0 as usize == 0 {
        return (0, 0);
    }
    let title = window_title(hwnd);
    (hwnd.0 as usize, fnv1a(&title))
}

/// alpha.14 — context of the CURRENT foreground window (pump-side
/// sampling; the main thread has no UIA state and this costs one
/// GetWindowTextW on a cached caption — no message is sent).
pub fn fg_identity_current() -> (usize, u64) {
    unsafe { fg_identity(GetForegroundWindow()) }
}

/// FNV-1a 64-bit — stable, allocation-free page-title fingerprint.
fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// First-contact warm-up (alpha.6): count the provider's REAL element
/// population and capture the foreground window rect (coordinate
/// ground truth for the log). The FindAll itself forces Chromium to
/// materialize the full accessibility tree — without it, the cached
/// walk only sees whatever fragment Chromium had handy.
unsafe fn warmup(automation: &IUIAutomation, fg: HWND) -> (u32, String) {
    let mut rect_line = String::new();
    let mut fg_rect = RECT::default();
    if GetWindowRect(fg, &mut fg_rect).is_ok() {
        rect_line = format!(
            "fg rect ({},{}),({},{}) {}x{}",
            fg_rect.left,
            fg_rect.top,
            fg_rect.right,
            fg_rect.bottom,
            fg_rect.right - fg_rect.left,
            fg_rect.bottom - fg_rect.top
        );
    }
    let Ok(root) = automation.ElementFromHandle(fg) else {
        return (0, rect_line);
    };
    let Ok(cond) = automation.CreateTrueCondition() else {
        return (0, rect_line);
    };
    let Ok(all) = root.FindAll(TreeScope_Descendants, &cond) else {
        return (0, rect_line);
    };
    let n = all.Length().unwrap_or(0).max(0) as u32;
    (n, rect_line)
}

// =====================================================================
// Tree walk
// =====================================================================

#[derive(Clone, Copy, PartialEq)]
enum MediaKind {
    Image,
    Link,
    Video,
}

#[derive(Clone)]
struct Candidate {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    dynamic: bool,
    /// Accessible name present (non-blank)? Named media = a real page
    /// item (titled thumbnail); unnamed media = a layer/tile of one
    /// rendered media. The merge tolerance keys on this — see gap_tol().
    named: bool,
    /// The accessible name itself, trimmed and truncated (24 chars) —
    /// SAME-NAME pairs merge at the scaled tolerance (alpha.9); blank
    /// = an unnamed fragment.
    name: String,
    /// "img" | "lnk" | "vid" — for the candidate observability dump.
    kind: &'static str,
}

struct WalkCtx {
    total: u32,
    docs: u32,
    /// Foreground window rect — candidates are clamped to it.
    fg: Option<RECT>,
    candidates: Vec<Candidate>,
}

/// Build the cache request and walk the fg tree — the cheap main path.
/// Returns None when the cache request or BuildUpdatedCache fails; the
/// caller then drops to the live walk.
unsafe fn cached_walk(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    fg: Option<RECT>,
) -> Option<WalkCtx> {
    let cache = automation.CreateCacheRequest().ok()?;
    let _ = cache.AddProperty(UIA_ControlTypePropertyId);
    let _ = cache.AddProperty(UIA_NamePropertyId);
    let _ = cache.AddProperty(UIA_LocalizedControlTypePropertyId);
    let _ = cache.AddProperty(UIA_BoundingRectanglePropertyId);
    let _ = cache.AddProperty(UIA_IsOffscreenPropertyId);
    let cached_root = root.BuildUpdatedCache(&cache).ok()?;
    let mut ctx = WalkCtx {
        total: 0,
        docs: 0,
        fg,
        candidates: Vec::new(),
    };
    walk_cached(&cached_root, false, 0, &mut ctx);
    Some(ctx)
}

/// Walk the foreground window and return the desired hole set.
/// `warm_total` is mutated: it re-baselines from every live walk (the
/// live FindAll sees the REAL population) and grows when the tree
/// outgrows the recorded warm-up size — both keep the starvation
/// heuristic honest without a re-warm-up.
///
/// Returns (specs, cand_dump, went_live, starved_suppressed):
///   went_live          — a live FindAll ran (caller restarts the
///                        cooldown clock);
///   starved_suppressed — starved + cooling down + not a wake walk:
///                        specs came back EMPTY and the caller holds the
///                        last trusted (sticky) set on screen instead.
unsafe fn discover(
    automation: &IUIAutomation,
    fg: HWND,
    warm_total: &mut u32,
    allow_live: bool,
    expedite: bool,
    settle_first: bool,
) -> (Vec<HoleSpec>, String, bool, bool) {
    // Foreground window rect — the clamp anchor for every candidate.
    let mut fg_rect = RECT::default();
    let fg_opt = if GetWindowRect(fg, &mut fg_rect).is_ok() {
        Some(fg_rect)
    } else {
        None
    };

    let Ok(root) = automation.ElementFromHandle(fg) else {
        return (Vec::new(), String::new(), false, false);
    };

    // Cached walk (the probe's proven main path).
    if let Some(ctx) = cached_walk(automation, &root, fg_opt) {
        // Baseline growth: the tree legitimately outgrew the
        // recorded warm-up population (lazy expansion, SPA
        // growth). Grow the baseline — otherwise every later walk
        // reads as "starved".
        if ctx.total > *warm_total {
            *warm_total = ctx.total;
        }
        // GECKO GUARD: Gecko/LibreWolf succeed at BuildUpdatedCache
        // but never fill CachedChildren — the walk sees only the
        // root element. Don't hand back an empty hole set then;
        // drop to the live walk, which works on every provider.
        if ctx.total > 1 {
            // STARVATION GUARD (alpha.6, split in alpha.12, bridged in
            // alpha.13): a cached walk that saw LESS THAN HALF the
            // warm-up population means the provider is handing out a
            // PARTIAL tree — its rects are placeholders and holes built
            // from them are the "random holes" report. Three outcomes:
            //   · live allowed (5s cooldown elapsed) → live walk now;
            //   · WAKE walk (scroll end / tab switch / F5-ON) → settle,
            //     re-walk, and if still starved run the live FindAll
            //     IMMEDIATELY — user-initiated freshness must not queue
            //     behind the idle cooldown;
            //   · otherwise → publish nothing and let the caller hold
            //     the last TRUSTED set on screen (sticky). Publishing
            //     empty here is what made YouTube's holes "go gone" in
            //     alpha.12 — the starved state is CHRONIC on heavy
            //     pages, not a one-tick hiccup.
            if *warm_total > 50 && ctx.total * 2 < *warm_total {
                if allow_live {
                    tracing::info!(
                        "tracker: cached walk starved ({}/{} nodes) — live walk this tick",
                        ctx.total,
                        *warm_total
                    );
                    let (specs, dump, total) = live_discover(automation, &root, ctx.fg, false);
                    *warm_total = total; // the live walk is ground truth
                    if STARVED_QUIET.swap(false, Ordering::Relaxed) {
                        tracing::info!("tracker: full tree restored ({total} nodes)");
                    }
                    return (specs, dump, true, false);
                }
                if expedite {
                    if settle_first {
                        // Tab switch / alt-tab: give the provider a
                        // moment to finish tearing down / rebuilding
                        // before re-walking (also the alpha.11 wedge
                        // window — walking it at +50ms was the risk).
                        std::thread::sleep(Duration::from_millis(SETTLE_MS));
                    }
                    if let Some(ctx2) = cached_walk(automation, &root, fg_opt) {
                        if ctx2.total > *warm_total {
                            *warm_total = ctx2.total;
                        }
                        if !(*warm_total > 50 && ctx2.total * 2 < *warm_total) {
                            // The tree settled into a full population
                            // while we waited — publish the cached set.
                            if STARVED_QUIET.swap(false, Ordering::Relaxed) {
                                tracing::info!(
                                    "tracker: tree back to full population ({} nodes)",
                                    ctx2.total
                                );
                            }
                            let (specs, dump) = ctx_to_specs(ctx2);
                            return (specs, dump, false, false);
                        }
                    }
                    tracing::info!("tracker: still starved after wake — expedited live walk");
                    let (specs, dump, total) = live_discover(automation, &root, ctx.fg, false);
                    *warm_total = total; // the live walk is ground truth
                    if STARVED_QUIET.swap(false, Ordering::Relaxed) {
                        tracing::info!("tracker: full tree restored ({total} nodes)");
                    }
                    return (specs, dump, true, false);
                }
                if !STARVED_QUIET.swap(true, Ordering::Relaxed) {
                    tracing::info!(
                        "tracker: cached walk starved ({}/{} nodes), live walk cooling down — holding the last trusted hole set (sticky) instead of partial-tree rects",
                        ctx.total,
                        *warm_total
                    );
                }
                return (Vec::new(), String::new(), false, true);
            }
            if STARVED_QUIET.swap(false, Ordering::Relaxed) {
                tracing::info!(
                    "tracker: tree back to full population ({} nodes)",
                    ctx.total
                );
            }
            let (specs, dump) = ctx_to_specs(ctx);
            return (specs, dump, false, false);
        }
        if !FALLBACK_LOGGED.swap(true, Ordering::Relaxed) {
            tracing::info!(
                "tracker: cached walk saw only {} node(s) — CachedChildren empty (Gecko) — permanent live fallback engaged",
                ctx.total
            );
        }
        // Cache failed OR cached walk came back empty → live walk.
    }

    // FALLBACK: flat live FindAll (no document scoping possible cheaply
    // — gates still apply). Same shape as the probe's fallback.
    tracing::debug!("tracker: cached walk unavailable — live fallback");
    let (specs, dump, total) = live_discover(automation, &root, fg_opt, true);
    *warm_total = total; // the live walk is ground truth
    (specs, dump, false, false)
}

/// Flat live FindAll walk: enumerate ALL descendants, classify each.
/// Works on every provider — never relies on CachedChildren.
/// `log_len` = mention the element count in the log (the permanent-
/// fallback path logs it; the starvation path already explained why).
unsafe fn live_discover(
    automation: &IUIAutomation,
    root: &IUIAutomationElement,
    fg: Option<RECT>,
    log_len: bool,
) -> (Vec<HoleSpec>, String, u32) {
    let Ok(true_cond) = automation.CreateTrueCondition() else {
        return (Vec::new(), String::new(), 0);
    };
    let Ok(all) = root.FindAll(TreeScope_Descendants, &true_cond) else {
        return (Vec::new(), String::new(), 0);
    };
    let Ok(len) = all.Length() else {
        return (Vec::new(), String::new(), 0);
    };
    if log_len {
        tracing::debug!("tracker: live walk: {len} elements");
    }
    let mut ctx = WalkCtx {
        total: 0,
        docs: 0,
        fg,
        candidates: Vec::new(),
    };
    for i in 0..len {
        if ctx.total > MAX_NODES {
            break;
        }
        if let Ok(el) = all.GetElement(i) {
            classify_live(&el, &mut ctx);
        }
    }
    let total = ctx.total;
    let (specs, dump) = ctx_to_specs(ctx);
    (specs, dump, total)
}

/// Cached-walk recursion. `in_doc` = we are inside a Document subtree —
/// only there do media candidates count (browser-chrome preservation).
fn walk_cached(el: &IUIAutomationElement, in_doc: bool, depth: u32, ctx: &mut WalkCtx) {
    unsafe {
        if ctx.total > MAX_NODES || depth > MAX_DEPTH {
            return;
        }
        ctx.total += 1;

        let ct = el.CachedControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
        let is_doc = ct == UIA_DocumentControlTypeId;
        if is_doc {
            ctx.docs += 1;
        }
        let in_doc_child = in_doc || is_doc;

        if in_doc && !is_doc {
            // OFFSCREEN HINT (alpha.14 — fail-open; was a hard filter in
            // alpha.12/13): IsOffscreen=true usually means display:none,
            // scrolled out, or a virtualized placeholder — stale or huge
            // rects. But Chromium also flags VISIBLE media offscreen on
            // some pages (the alpha.13 "no hole in youtube" report: the
            // player's IMAGE layers were suppressed on BOTH walk paths,
            // and the video-only canary could not even log it). The flag
            // is now a HINT: the candidate is still offered when its
            // geometry is independently usable (offscreen_trust) — the
            // fg clamp, size gates, giant filter and merge all re-judge
            // the rect on its own merits. Recursion continues — every
            // element is judged on its own flag. Cached reads are
            // bundle-local: the unconditional lct/name/rect reads here
            // are free.
            let offscreen = el.CachedIsOffscreen().is_ok_and(|b| b.as_bool());
            let lct = el
                .CachedLocalizedControlType()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let name = el.CachedName().map(|s| s.to_string()).unwrap_or_default();
            let rect = el.CachedBoundingRectangle().unwrap_or_default();
            let kind = classify(ct, &lct);
            if !offscreen || offscreen_trust(kind, &rect, ctx.fg) {
                if offscreen {
                    note_offscreen_admit(kind, &rect);
                }
                offer_candidate(kind, &rect, &name, ctx);
            }
        }

        if ctx.total < MAX_NODES {
            if let Ok(kids) = el.GetCachedChildren() {
                if let Ok(len) = kids.Length() {
                    for i in 0..len {
                        if ctx.total > MAX_NODES {
                            break;
                        }
                        if let Ok(kid) = kids.GetElement(i) {
                            walk_cached(&kid, in_doc_child, depth + 1, ctx);
                        }
                    }
                }
            }
        }
    }
}

/// Live-walk classification for the fallback path (flat list — no
/// recursion, no document scoping).
fn classify_live(el: &IUIAutomationElement, ctx: &mut WalkCtx) {
    unsafe {
        ctx.total += 1;
        // OFFSCREEN HINT (alpha.14 — see walk_cached / offscreen_trust).
        // The live path is where YouTube's candidates actually come
        // from (chronically starved cached tree), so the fail-open
        // matters MOST here. Offscreen elements pay the extra Current
        // reads only when the flag is set; the onscreen majority pays
        // one extra property read over the alpha.12 code.
        if el.CurrentIsOffscreen().is_ok_and(|b| b.as_bool()) {
            let rect = el.CurrentBoundingRectangle().unwrap_or_default();
            let ct = el.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
            let lct = el
                .CurrentLocalizedControlType()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let kind = classify(ct, &lct);
            if !offscreen_trust(kind, &rect, ctx.fg) {
                return;
            }
            note_offscreen_admit(kind, &rect);
            let name = el.CurrentName().map(|s| s.to_string()).unwrap_or_default();
            offer_candidate(kind, &rect, &name, ctx);
            return;
        }
        let ct = el.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
        let lct = el
            .CurrentLocalizedControlType()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let name = el.CurrentName().map(|s| s.to_string()).unwrap_or_default();
        let rect = el.CurrentBoundingRectangle().unwrap_or_default();
        offer_candidate(classify(ct, &lct), &rect, &name, ctx);
    }
}

/// Locale-proof classification: numeric control types first, localized
/// string only for types UIA has no number for (video, graphic).
fn classify(ct: UIA_CONTROLTYPE_ID, lct: &str) -> Option<MediaKind> {
    if ct == UIA_ImageControlTypeId {
        return Some(MediaKind::Image);
    }
    if ct == UIA_HyperlinkControlTypeId {
        return Some(MediaKind::Link);
    }
    let l = lct.to_ascii_lowercase();
    if l == "video" {
        return Some(MediaKind::Video);
    }
    if l == "graphic" {
        // Chromium reports <svg> & co. as "graphic" — behaves like image.
        return Some(MediaKind::Image);
    }
    // Some providers only fill the localized string ("image"/"link").
    if l == "image" {
        return Some(MediaKind::Image);
    }
    if l == "link" {
        return Some(MediaKind::Link);
    }
    None
}

/// alpha.14 — geometric trust decision for IsOffscreen=true elements.
/// Chromium's offscreen bit lies for visible media (custom players,
/// z-order quirks) — the alpha.12/13 hard gate silently ate the
/// YouTube player on BOTH walk paths. The gate is now a hint; an
/// offscreen-flagged element is still offered when its geometry is
/// independently usable:
///   · empty rect → unusable (offer_candidate would reject it anyway);
///   · fully outside the fg window → unusable (scrolled-out garbage —
///     the clamp would kill it too, this just skips the reads);
///   · video-kind → trusted (the player itself — offer_candidate's
///     64px gate and the clamp still apply);
///   · anything else → trusted only when LARGE: both axes at least a
///     quarter of the fg window's smaller dimension (≥270px on 1080p).
///     Stale virtualized-list thumbnails — the alpha.12 "random holes"
///     generator — are far smaller and stay suppressed.
fn offscreen_trust(kind: Option<MediaKind>, rect: &RECT, fg: Option<RECT>) -> bool {
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    if w <= 0 || h <= 0 {
        return false;
    }
    let Some(fg) = fg else {
        return true; // no anchor to judge against — let the clamp decide
    };
    let outside = rect.right <= fg.left
        || rect.left >= fg.right
        || rect.bottom <= fg.top
        || rect.top >= fg.bottom;
    if outside {
        return false;
    }
    if kind == Some(MediaKind::Video) {
        return true;
    }
    let bar = ((fg.right - fg.left).min(fg.bottom - fg.top) / 4).max(MIN_MEDIA);
    w >= bar && h >= bar
}

/// alpha.14 — counter + one-line canary for the fail-open path (see
/// offscreen_trust). The count lands in the diagnostics report; the
/// detail line logs once per process.
fn note_offscreen_admit(kind: Option<MediaKind>, rect: &RECT) {
    OFFSCREEN_ADMITS.fetch_add(1, Ordering::Relaxed);
    if !OFFSCREEN_ADMIT_LOGGED.swap(true, Ordering::Relaxed) {
        let kind_tag = match kind {
            Some(MediaKind::Video) => "video",
            Some(MediaKind::Image) => "image",
            Some(MediaKind::Link) => "link",
            None => "?",
        };
        tracing::warn!(
            "tracker: offscreen-flagged {} element ADMITTED by geometry ({}x{} @({},{})) — Chromium's IsOffscreen flag disputed; if holes appear where none should, this path is why",
            kind_tag,
            rect.right - rect.left,
            rect.bottom - rect.top,
            rect.left,
            rect.top
        );
    }
}

/// Size gates + fg-window clamp + 4px quantize + zero-rect rejection.
/// `name` = the element's accessible name (blank = unnamed layer).
/// Physical pixels end to end.
fn offer_candidate(kind: Option<MediaKind>, rect: &RECT, name: &str, ctx: &mut WalkCtx) {
    let Some(kind) = kind else { return };
    let kind_tag = match kind {
        MediaKind::Image => "img",
        MediaKind::Link => "lnk",
        MediaKind::Video => "vid",
    };
    let named = !name.trim().is_empty();
    // Keep the name for the same-name merge rule: trimmed, truncated
    // to 24 chars (locale-safe — chars, never byte-sliced).
    let name: String = name.trim().chars().take(24).collect();
    let mut x = rect.left;
    let mut y = rect.top;
    let mut w = rect.right - rect.left;
    let mut h = rect.bottom - rect.top;
    if w <= 0 || h <= 0 {
        return;
    }

    // Clamp to the foreground window. Media renders inside it, and
    // off-screen garbage rects (probe samples have shown y=-11526)
    // used to become holes in nowhere-land — partially visible ones
    // landed as "a random rectangle on top of whatever was there".
    if let Some(fg) = ctx.fg {
        let l = x.max(fg.left);
        let t = y.max(fg.top);
        let r = (x + w).min(fg.right);
        let b = (y + h).min(fg.bottom);
        if r - l <= 0 || b - t <= 0 {
            return;
        }
        x = l;
        y = t;
        w = r - l;
        h = b - t;
    }

    // Quantize to a 4px grid (floor origin, extend size). UIA rects
    // wobble a few px between walks; unquantized, every wobble moved
    // the hole a little and re-armed the Magnifier capture — edge
    // flicker plus pointless pipeline resets inside video.
    let xq = x - x.rem_euclid(QUANTUM);
    let yq = y - y.rem_euclid(QUANTUM);
    let rq = (x + w) + (QUANTUM - (x + w).rem_euclid(QUANTUM)) % QUANTUM;
    let bq = (y + h) + (QUANTUM - (y + h).rem_euclid(QUANTUM)) % QUANTUM;
    x = xq;
    y = yq;
    w = rq - xq;
    h = bq - yq;

    let passes = match kind {
        // alpha.6: image/graphic AND video gates raised 40/48 → 64 px.
        // Real media is ≥64 px on both axes in practice; every 40–63 px
        // UI icon (comment avatars, player-overlay buttons, app chrome)
        // used to clear the old bar and punch the "small squares
        // scattered on UI elements" the user kept reporting.
        MediaKind::Image => w >= MIN_MEDIA && h >= MIN_MEDIA,
        MediaKind::Link => w >= MIN_LINK && h >= MIN_LINK,
        MediaKind::Video => w >= MIN_MEDIA && h >= MIN_MEDIA,
    };
    if !passes {
        return;
    }
    ctx.candidates.push(Candidate {
        x,
        y,
        w,
        h,
        dynamic: kind == MediaKind::Video,
        named,
        name,
        kind: kind_tag,
    });
}

// =====================================================================
// Candidates → HoleSpecs (merge + cap)
// =====================================================================

impl Candidate {
    /// Gap tolerance for the pair (alpha.9 — SAME-NAME):
    /// · both candidates unnamed (player canvas/poster/tile layers), OR
    ///   both named with the IDENTICAL name (fragments of one titled
    ///   media — alpha.8 sent every named pair to the 8px floor and the
    ///   player fragmented again):
    ///   tolerance = max(8px, 6.25% of the smaller fragment's larger
    ///   axis). Seams up to ~45px on a 720p media merge.
    /// · any other pairing (one named, or two DIFFERENT names — grid
    ///   thumbnails carry distinct titles): the 8px floor. Different
    ///   names must never chain across a page gutter — that chain is
    ///   exactly the alpha.7 "one hole covers everything" regression.
    fn gap_tol(&self, o: &Candidate) -> i32 {
        let both_unnamed = !self.named && !o.named;
        let same_name = self.named && o.named && self.name == o.name;
        if both_unnamed || same_name {
            let smaller_max = self.w.max(self.h).min(o.w.max(o.h));
            return MERGE_GAP.max(smaller_max / 16);
        }
        MERGE_GAP
    }

    /// Gap-tolerant proximity: two rects whose edges come within the
    /// pair's gap tolerance belong to the same media. Strict `<` keeps
    /// the floor honest: a gap of exactly MERGE_GAP never merges.
    fn near(&self, o: &Candidate) -> bool {
        let g = self.gap_tol(o);
        self.x - g < o.x + o.w
            && o.x < self.x + self.w + g
            && self.y - g < o.y + o.h
            && o.y < self.y + self.h + g
    }

    /// Union rect of two candidates (dynamic = either; the cluster
    /// name keeps the first non-blank — only the DUMP ever reads it,
    /// pairing decisions run on the pre-merge candidates).
    fn union(self, o: Candidate) -> Candidate {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let r = (self.x + self.w).max(o.x + o.w);
        let b = (self.y + self.h).max(o.y + o.h);
        let name = if self.name.is_empty() {
            o.name
        } else {
            self.name
        };
        Candidate {
            x,
            y,
            w: r - x,
            h: b - y,
            dynamic: self.dynamic || o.dynamic,
            named: self.named || o.named,
            name,
            kind: if self.dynamic { self.kind } else { o.kind },
        }
    }

    fn area(&self) -> i64 {
        self.w as i64 * self.h as i64
    }
}

fn ctx_to_specs(ctx: WalkCtx) -> (Vec<HoleSpec>, String) {
    if ctx.candidates.is_empty() {
        return (Vec::new(), String::new());
    }

    // CANDIDATE DUMP (alpha.6, name-tagged in alpha.9): the gated
    // candidate set BEFORE merging — kind, `*` = named, `[name…]` = the
    // accessible name's first 6 chars (the same-name merge rule lives
    // or dies on this), size, position. This is what turns the next
    // "holes are in the wrong place" report into a precise diagnosis:
    // the log will show exactly which element produced which rect.
    let mut dump = String::new();
    for (i, c) in ctx.candidates.iter().enumerate().take(12) {
        let name_tag = if c.named {
            let head: String = c.name.chars().take(6).collect();
            format!("[{head}…]")
        } else {
            String::new()
        };
        dump.push_str(&format!(
            "{}{}{}{} {}x{}@({},{})  ",
            c.kind,
            if c.named { "*" } else { "" },
            i,
            name_tag,
            c.w,
            c.h,
            c.x,
            c.y
        ));
    }
    if ctx.candidates.len() > 12 {
        dump.push_str(&format!("+{}", ctx.candidates.len() - 12));
    }

    // GIANT FILTER (alpha.8): a NON-dynamic candidate covering ≥ 85%
    // of the foreground window is clamped garbage (probe history has
    // full-window rects from virtualized UIA reads) or a page wrapper
    // mislabeled as media. Whatever its origin, it would absorb the
    // entire clustering into one window-sized hole. Real media that
    // big is a fullscreen VIDEO — and that arrives dynamic, which this
    // filter keeps.
    let mut cands = ctx.candidates;
    if let Some(fg) = ctx.fg {
        let fg_area = (fg.right - fg.left) as f64 * (fg.bottom - fg.top) as f64;
        if fg_area > 0.0 {
            let before = cands.len();
            cands.retain(|c| c.dynamic || (c.area() as f64) < GIANT_SHARE * fg_area);
            if cands.len() != before {
                tracing::info!(
                    "tracker: giant filter dropped {} window-sized candidate(s)",
                    before - cands.len()
                );
            }
        }
    }
    if cands.is_empty() {
        return (Vec::new(), dump);
    }

    // UNION-FIND CLUSTERING (alpha.7, name-gated in alpha.8): every
    // pair of candidates within the pair's gap tolerance joins the
    // same cluster; clusters are transitive (A~B, B~C ⇒ one media),
    // so a player reported as any number of UNNAMED fragments with
    // any seam up to the scaled tolerance collapses into exactly ONE
    // hole. NAMED candidates never leave the 8px floor, so titled
    // thumbnails cannot chain across page gutters (the alpha.7
    // mega-hole). No union-growth cap: fragments of one media may
    // union to any size.
    let n = cands.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]]; // path halving
            i = parent[i];
        }
        i
    }
    for i in 0..n {
        for j in (i + 1)..n {
            let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
            if ri != rj && cands[i].near(&cands[j]) {
                parent[rj] = ri;
            }
        }
    }
    let mut clusters: Vec<Option<Candidate>> = cands.into_iter().map(Some).collect();
    for i in 0..n {
        let r = find(&mut parent, i);
        if r != i {
            let frag = clusters[i].take();
            if let Some(f) = frag {
                // Candidate is no longer Copy (it carries the accessible
                // name) — take/replace instead of Option::map.
                if let Some(acc) = clusters[r].take() {
                    clusters[r] = Some(acc.union(f));
                }
            }
        }
    }
    let mut kept: Vec<Candidate> = clusters.into_iter().flatten().collect();

    // Degenerate slivers out.
    kept.retain(|k| k.w >= MIN_HOLE && k.h >= MIN_HOLE);

    // Deterministic order — top-to-bottom, left-to-right. An unchanged
    // hole set therefore keeps its index mapping into the pump's
    // window pool (no hole-window churn when specs merely swap slots).
    kept.sort_by_key(|k| (k.y, k.x, k.w, k.h));

    // Hard cap, largest first (order restored afterwards).
    if kept.len() > MAX_HOLES {
        kept.sort_by_key(|k| std::cmp::Reverse(k.area()));
        kept.truncate(MAX_HOLES);
        kept.sort_by_key(|k| (k.y, k.x, k.w, k.h));
    }

    let specs = kept
        .into_iter()
        .map(|k| HoleSpec {
            x: k.x,
            y: k.y,
            w: k.w,
            h: k.h,
            dynamic: k.dynamic,
        })
        .collect();
    (specs, dump)
}
