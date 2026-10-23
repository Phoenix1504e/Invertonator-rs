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
//     short the moment a scroll burst ends, a wheel fires, or the
//     foreground window moves/resizes/changes — the next walk happens
//     ~50ms later, whatever the backoff was. The starvation live-walk
//     cooldown stays widened 2s → 5s (alpha.9).
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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Descendants,
    UIA_BoundingRectanglePropertyId, UIA_ControlTypePropertyId, UIA_DocumentControlTypeId,
    UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId, UIA_LocalizedControlTypePropertyId,
    UIA_NamePropertyId, UIA_CONTROLTYPE_ID,
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

/// One-shot flag: the Gecko live-fallback notice is logged exactly once
/// per process (discovery ticks every 400ms — per-tick logs would spam
/// log.txt at 150 lines/minute).
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

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
    // Warm-up + live-walk state (alpha.6): the warm-up runs once per
    // foreground window; the starvation live-walk is throttled.
    let mut warm_fg: usize = 0;
    let mut warm_total: u32 = 0;
    let mut last_live = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if enabled.load(Ordering::Relaxed) {
            let fg = unsafe { GetForegroundWindow() };
            let t0 = std::time::Instant::now();
            let (specs, cand_dump) = if fg.is_invalid() {
                (Vec::new(), String::new())
            } else {
                // WARM-UP (alpha.6): first contact with a foreground
                // window — force the provider to materialize its full
                // accessibility tree and record how big it really is.
                // Chromium hands out a PARTIAL tree otherwise, and the
                // tracker then only ever saw the player-adjacent part
                // ("media outside the player is not recognised").
                if fg.0 as usize != warm_fg {
                    warm_fg = fg.0 as usize;
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
                let (s, dump, went_live) =
                    unsafe { discover(&automation, fg, warm_total, allow_live) };
                if went_live {
                    last_live = std::time::Instant::now();
                }
                (s, dump)
            };
            let walk_ms = t0.elapsed().as_millis();
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
            holes.set_specs(specs);

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
            let sig = format!("{}:{had_specs}", fg.0 as usize);
            if sig != last_sig || last_beat.elapsed() >= std::time::Duration::from_secs(10) {
                tracing::info!(
                    "tracker: fg=\"{}\" holes={n_holes} walk={walk_ms}ms cadence={interval}ms",
                    window_title(fg)
                );
                last_sig = sig;
                last_beat = std::time::Instant::now();
            }
        } else if had_specs {
            // Disabled while holes were out — clear once.
            holes.set_specs(Vec::new());
            had_specs = false;
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
                    }
                    was_hold = false;
                }
                if !wake {
                    let w = unsafe { GetForegroundWindow() };
                    let wu = w.0 as usize;
                    if last_fg != 0 && wu != last_fg {
                        wake = true;
                    } else if wu == last_fg && !w.is_invalid() {
                        let mut r = RECT::default();
                        if unsafe { GetWindowRect(w, &mut r) }.is_ok()
                            && (r.left != last_fg_rect.left
                                || r.top != last_fg_rect.top
                                || r.right != last_fg_rect.right
                                || r.bottom != last_fg_rect.bottom)
                        {
                            wake = true;
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

/// Walk the foreground window and return the desired hole set.
unsafe fn discover(
    automation: &IUIAutomation,
    fg: HWND,
    warm_total: u32,
    allow_live: bool,
) -> (Vec<HoleSpec>, String, bool) {
    // Foreground window rect — the clamp anchor for every candidate.
    let mut fg_rect = RECT::default();
    let fg_opt = if GetWindowRect(fg, &mut fg_rect).is_ok() {
        Some(fg_rect)
    } else {
        None
    };

    let Ok(root) = automation.ElementFromHandle(fg) else {
        return (Vec::new(), String::new(), false);
    };

    // Cached walk (the probe's proven main path).
    if let Ok(cache) = automation.CreateCacheRequest() {
        let _ = cache.AddProperty(UIA_ControlTypePropertyId);
        let _ = cache.AddProperty(UIA_NamePropertyId);
        let _ = cache.AddProperty(UIA_LocalizedControlTypePropertyId);
        let _ = cache.AddProperty(UIA_BoundingRectanglePropertyId);
        if let Ok(cached_root) = root.BuildUpdatedCache(&cache) {
            let mut ctx = WalkCtx {
                total: 0,
                docs: 0,
                fg: fg_opt,
                candidates: Vec::new(),
            };
            walk_cached(&cached_root, false, 0, &mut ctx);
            // GECKO GUARD: Gecko/LibreWolf succeed at BuildUpdatedCache
            // but never fill CachedChildren — the walk sees only the
            // root element. Don't hand back an empty hole set then;
            // drop to the live walk, which works on every provider.
            if ctx.total > 1 {
                // STARVATION GUARD (alpha.6): a cached walk that saw
                // LESS THAN HALF the warm-up population means the
                // provider is still handing out a partial tree (the
                // Chromium lazy-materialization case). One live FindAll
                // walk fixes the hole set for this tick; the 2s
                // cooldown keeps the tax off the video renderer.
                if warm_total > 50 && ctx.total * 2 < warm_total && allow_live {
                    tracing::info!(
                        "tracker: cached walk starved ({}/{} nodes) — live walk this tick",
                        ctx.total,
                        warm_total
                    );
                    let (specs, dump) = live_discover(automation, &root, ctx.fg, false);
                    return (specs, dump, true);
                }
                let (specs, dump) = ctx_to_specs(ctx);
                return (specs, dump, false);
            }
            if !FALLBACK_LOGGED.swap(true, Ordering::Relaxed) {
                tracing::info!(
                    "tracker: cached walk saw only {} node(s) — CachedChildren empty (Gecko) — permanent live fallback engaged",
                    ctx.total
                );
            }
        }
        // Cache failed OR cached walk came back empty → live walk.
    }

    // FALLBACK: flat live FindAll (no document scoping possible cheaply
    // — gates still apply). Same shape as the probe's fallback.
    tracing::debug!("tracker: cached walk unavailable — live fallback");
    let (specs, dump) = live_discover(automation, &root, fg_opt, true);
    (specs, dump, false)
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
) -> (Vec<HoleSpec>, String) {
    let Ok(true_cond) = automation.CreateTrueCondition() else {
        return (Vec::new(), String::new());
    };
    let Ok(all) = root.FindAll(TreeScope_Descendants, &true_cond) else {
        return (Vec::new(), String::new());
    };
    let Ok(len) = all.Length() else {
        return (Vec::new(), String::new());
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
    ctx_to_specs(ctx)
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
            let lct = el
                .CachedLocalizedControlType()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let name = el.CachedName().map(|s| s.to_string()).unwrap_or_default();
            let rect = el.CachedBoundingRectangle().unwrap_or_default();
            offer_candidate(classify(ct, &lct), &rect, &name, ctx);
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
