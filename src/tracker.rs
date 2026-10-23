// Invertonator-rs — tracker: media classification gates → live holes
//
//   Runs a discovery loop on its own thread: it walks the FOREGROUND
//   window's UIA tree (cached walk, live fallback) and turns media
//   elements into HoleSpecs for the hole pump.
//
//   CADENCE (alpha.7 — adaptive): the walk starts at DISCOVERY_MS and
//   doubles while the foreground window AND its hole set stay stable,
//   capped at BACKOFF_MAX_MS. Every change — app switch, title change
//   (navigation), different hole rects, recent wheel activity — snaps
//   the cadence back to fast. A playing video holds a static rect, so
//   steady-state re-scans drop 4×; that plus the FindAllBuildCache
//   bulk walk below is what fixed the "video is very laggy" field
//   report (the tracker used to hammer the browser's UI thread with
//   tens of thousands of cross-process COM calls per second).
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
//   · Size gate (alpha.8, UNIFIED): every kind needs ≥ 80×80 px —
//     kills favicons, inline icons, avatar squares AND the 1×1
//     ad-tracking <video> elements (videos previously passed at ANY
//     size, so a hidden ad clip could anchor a square hole); wide
//     text links (short bounding boxes) stay inverted.
//   · PER-KIND DOMINANCE (alpha.8): within each kind (video / image /
//     link), keep only candidates within 20% of the largest candidate
//     OF THAT KIND. The hard alpha.6 "video priority" — any <video>
//     on the page meant images/links got NOTHING — is what the
//     YouTube field report called "other media are not recognised":
//     the watch page's player kept its hole while every sidebar
//     thumbnail was dropped. Per-kind keeps the player AND its
//     thumbnails, and still kills the avatar/icon swarms (they sit
//     below the 80px gate or far below their kind's largest).
//   · Videos sort FIRST before the hole cap, so a capped page keeps
//     its player before its thumbnails.
//   · FLATNESS PROBE: image candidates whose on-screen pixels are
//     uniform (solid spacer / bogus rect parked on flat page area)
//     get no hole — 5×5 GetPixel grid, inverted-desktop-safe.
//   · Overlap merge (TIGHTENED): an <img> inside an <a> produces two
//     candidates for the same thumbnail — merged into one hole, but
//     ONLY when the intersection is ≥ 40% of the smaller candidate.
//     The old any-touch rule grew Frankenstein unions out of rects
//     that merely shared a corner (the "random rectangle" report).
//   · Cap at 4 holes, largest first.
//
//   COORDINATES: screen physical pixels end to end (per-monitor-v2
//   aware — see main::set_dpi_awareness).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement,
    TreeScope_Descendants, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId,
    UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId,
    UIA_LocalizedControlTypePropertyId, UIA_NamePropertyId,
    UIA_CONTROLTYPE_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetSystemMetrics, GetWindowTextW, GetWindowRect,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use crate::holes::{HoleSpec, Holes, KIND_IMG, KIND_LINK, KIND_VIDEO};

const DISCOVERY_MS: u64 = 400;  // fast floor after any change
/// Steady-state cap: while fg window + hole set are stable, the walk
/// interval doubles each tick up to this (4× fewer UIA passes over a
/// playing video).
const BACKOFF_MAX_MS: u64 = 1_600;
/// Wheel activity within this window forces the fast cadence — scroll
/// moves every media rect on screen (scrollhook::ms_since_wheel).
const WHEEL_FAST_MS: u64 = 1_500;
/// Backoff sleep granularity: the nap wakes early on foreground
/// switches / wheel activity, so snappiness survives the backoff.
const NAP_SLICE_MS: u64 = 100;
const MIN_MEDIA: i32 = 80;   // unified min w and h for EVERY kind (px)
const MAX_NODES: u32 = 100_000;
const MAX_DEPTH: u32 = 25;
const POKE_TOLERANCE: i32 = 8;   // pt beyond the window edge → bogus rect
const KEEP_MIN_PCT: i64 = 20;    // dominance: ≥ 20% of largest survivor's area
const MAX_KEEP: usize = 8;       // holes actually kept per tick (pool is 16)
const SUMMARY_EVERY_MS: u64 = 2_000;  // state-change summary throttle
const HEARTBEAT_MS: u64 = 10_000;     // unconditional liveness line

/// One-shot flag: the Gecko live-fallback notice is logged exactly once
/// per process (discovery ticks every 400ms–1.6s — per-tick logs would
/// spam log.txt).
static FALLBACK_LOGGED: AtomicBool = AtomicBool::new(false);

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

        Self { holes, enabled, stop, thread: Some(thread) }
    }

    /// true = discover holes for the foreground window. false = clear
    /// and idle. Called from apply_mode (holes exist in Invert only).
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
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
    let automation: IUIAutomation = match unsafe {
        CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    } {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("tracker: CoCreateInstance failed: {e} — tracker dead");
            return;
        }
    };
    tracing::info!(
        "tracker: alive (discovery {DISCOVERY_MS}ms → backoff {BACKOFF_MAX_MS}ms on stable state)"
    );

    let mut had_specs = false;
    let mut last_log = std::time::Instant::now();
    let mut last_key = String::new();
    // Adaptive cadence state — see the cadence constants above.
    let mut interval = DISCOVERY_MS;
    let mut last_sig = String::new();
    let mut last_fg: isize = 0;
    let mut last_title = String::new();

    while !stop.load(Ordering::Relaxed) {
        if enabled.load(Ordering::Relaxed) {
            let fg = unsafe { GetForegroundWindow() };
            let fg_raw = fg.0 as isize;
            let title = if fg.is_invalid() {
                String::new()
            } else {
                // SAFETY: fg valid, plain HWND read (GetWindowTextW).
                unsafe { window_title(fg) }
            };
            // A foreground switch — or even a title change, which is
            // how browsers announce navigation — makes the old hole
            // set suspect: snap back to the fast cadence.
            let fg_changed = fg_raw != last_fg || title != last_title;
            last_fg = fg_raw;
            last_title = title.clone();

            let t0 = std::time::Instant::now();
            let (specs, nodes, cands, docs_n, kinds) = if fg.is_invalid() {
                (Vec::new(), 0, 0, 0, [0u32; 3])
            } else {
                unsafe { discover(&automation, fg) }
            };
            let walk_ms = t0.elapsed().as_millis() as u64;
            had_specs = !specs.is_empty();
            holes.set_specs(specs.clone());

            // Cadence: identical hole set on an unchanged foreground →
            // double the interval (cap BACKOFF_MAX_MS); any change →
            // back to DISCOVERY_MS. UIA round-off is absorbed by the
            // 2px signature quantization.
            let sig = specs_sig(&specs);
            let stable = !fg_changed && sig == last_sig;
            last_sig = sig;
            interval = if stable {
                (interval * 2).min(BACKOFF_MAX_MS)
            } else {
                DISCOVERY_MS
            };

            // Throttled discovery summary — the alpha.3 log proved the
            // PROBE sees media but said nothing about what the TRACKER
            // produced. This line closes that observability gap: one
            // line per state change, plus a 10s heartbeat. iv=/walk=
            // expose the cadence and per-walk cost in the field log
            // (alpha.7 lag triage).
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_log).as_millis() as u64;
            let key = format!("{}\u{1}{}", title, specs.len());
            if elapsed >= HEARTBEAT_MS || (elapsed >= SUMMARY_EVERY_MS && key != last_key) {
                last_log = now;
                last_key = key;
                let rects: Vec<String> = specs
                    .iter()
                    .take(6)
                    .map(|s| {
                        format!(
                            "({},{}) {}x{}{}",
                            s.x, s.y, s.w, s.h,
                            if s.dynamic { " dyn" } else { "" }
                        )
                    })
                    .collect();
                tracing::info!(
                    "tracker: fg=\"{}\" nodes={} cands={} v={} i={} l={} docs={} holes={} iv={}ms walk={}ms [{}]",
                    title,
                    nodes,
                    cands,
                    kinds[2],
                    kinds[0],
                    kinds[1],
                    docs_n,
                    specs.len(),
                    interval,
                    walk_ms,
                    rects.join(" ")
                );
            }
        } else if had_specs {
            // Disabled while holes were out — clear once.
            holes.set_specs(Vec::new());
            had_specs = false;
        }
        nap(&stop, &enabled, interval, last_fg);
    }
    tracing::info!("tracker: stopped");
}

/// Sleeps up to `ms` in NAP_SLICE_MS slices, waking early when the
/// user switches windows, scrolls (recent wheel activity), toggles
/// the tracker, or the process stops. This is what keeps the backoff
/// safe: a backed-off tracker still reacts to alt-tab and scrolling
/// within one slice — both events demand fresh hole rects.
fn nap(stop: &AtomicBool, enabled: &AtomicBool, ms: u64, fg: isize) {
    let start_enabled = enabled.load(Ordering::Relaxed);
    let mut slept = 0u64;
    while slept < ms {
        std::thread::sleep(Duration::from_millis(NAP_SLICE_MS));
        slept += NAP_SLICE_MS;
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if enabled.load(Ordering::Relaxed) != start_enabled {
            return; // toggled mid-nap — react immediately
        }
        if !start_enabled {
            continue; // disabled: stay idle, don't wake on input
        }
        if crate::scrollhook::ms_since_wheel() < WHEEL_FAST_MS {
            return; // scrolling — hole rects are moving
        }
        let cur = unsafe { GetForegroundWindow() };
        if cur.0 as isize != fg {
            return; // app switch — walk the new foreground now
        }
    }
}

/// Change signature of a hole set — rects quantized to 2px (UIA
/// round-off must not read as a state change) plus the kind letter.
fn specs_sig(specs: &[HoleSpec]) -> String {
    let mut s = String::new();
    for sp in specs {
        s.push_str(&format!(
            "{},{},{},{},{};",
            sp.x / 2,
            sp.y / 2,
            sp.w / 2,
            sp.h / 2,
            sp.kind_char()
        ));
    }
    s
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

impl MediaKind {
    /// Index into the per-kind dominance table / kind-count log.
    fn slot(self) -> usize {
        match self {
            MediaKind::Image => 0,
            MediaKind::Link => 1,
            MediaKind::Video => 2,
        }
    }
}

struct Candidate {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// Size gate this candidate must keep passing after on-screen
    /// clamping (MIN_MEDIA for every kind).
    min: i32,
    kind: MediaKind,
}

impl Candidate {
    fn area(&self) -> i64 {
        self.w as i64 * self.h as i64
    }
    fn is_video(&self) -> bool {
        self.kind == MediaKind::Video
    }
    fn kind_id(&self) -> u8 {
        match self.kind {
            MediaKind::Video => KIND_VIDEO,
            MediaKind::Link => KIND_LINK,
            MediaKind::Image => KIND_IMG,
        }
    }
}

/// Intersection / smaller-area ratio — the tightened merge test.
fn overlap_ratio(a: &Candidate, b: &Candidate) -> f32 {
    let ix = a.x.max(b.x);
    let iy = a.y.max(b.y);
    let ir = (a.x + a.w).min(b.x + b.w);
    let ib = (a.y + a.h).min(b.y + b.h);
    let inter = (ir - ix).max(0) as i64 * (ib - iy).max(0) as i64;
    let min_area = a.area().min(b.area());
    if min_area <= 0 {
        return 0.0;
    }
    inter as f32 / min_area as f32
}

struct WalkCtx {
    total: u32,
    docs: u32,
    /// Screen rects of every Document element seen this walk — the
    /// live walk's document-pane scoping (browser page area).
    docs_rects: Vec<RECT>,
    candidates: Vec<Candidate>,
}

/// Walk the foreground window and return the desired hole set plus
/// walk counters for the summary log (specs, nodes, candidates, docs).
unsafe fn discover(
    automation: &IUIAutomation,
    fg: HWND,
) -> (Vec<HoleSpec>, u32, u32, u32, [u32; 3]) {
    let Ok(root) = automation.ElementFromHandle(fg) else {
        return (Vec::new(), 0, 0, 0, [0; 3]);
    };
    // Hole rects are clamped to the foreground window ∩ virtual screen
    // (see clip_rect). Computed once per discovery tick.
    let clip = clip_rect(fg);

    // ONE cache request, shared by the cached walk and the live
    // fallback. alpha.7: the live fallback now goes through
    // FindAllBuildCache — every element's properties are filled in ONE
    // cross-process pass, instead of 3–4 Current* round trips PER NODE
    // (LibreWolf: ~4k nodes × 4 reads at the old 400ms cadence ≈ tens
    // of thousands of COM calls per second straight into the browser's
    // UI thread — the video-lag driver).
    let cache = automation.CreateCacheRequest().ok();
    if let Some(c) = &cache {
        for p in [
            UIA_ControlTypePropertyId,
            UIA_NamePropertyId,
            UIA_LocalizedControlTypePropertyId,
            UIA_BoundingRectanglePropertyId,
        ] {
            let _ = c.AddProperty(p);
        }
    }

    // Cached walk (the probe's proven main path).
    if let Some(cache_req) = &cache {
        if let Ok(cached_root) = root.BuildUpdatedCache(cache_req) {
            let mut ctx = WalkCtx {
                total: 0,
                docs: 0,
                docs_rects: Vec::new(),
                candidates: Vec::new(),
            };
            walk_cached(&cached_root, false, 0, &mut ctx);
            // GECKO GUARD: Gecko/LibreWolf succeed at BuildUpdatedCache
            // but never fill CachedChildren — the walk sees only the
            // root element. Don't hand back an empty hole set then;
            // drop to the live walk, which works on every provider.
            if ctx.total > 1 {
                let nodes = ctx.total;
                let cands = ctx.candidates.len() as u32;
                let docs_n = ctx.docs;
                let (specs, kinds) = ctx_to_specs(ctx, clip);
                return (specs, nodes, cands, docs_n, kinds);
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

    // FALLBACK: flat live walk — no tree recursion possible, but
    // Document elements harvested during the walk give the same
    // document-pane scoping the cached path had (see classify_live +
    // clamp_candidate), and the size gates still apply.
    tracing::debug!("tracker: cached walk unavailable — live fallback");
    let Ok(true_cond) = automation.CreateTrueCondition() else {
        return (Vec::new(), 0, 0, 0, [0; 3]);
    };
    // Bulk cache build first; plain FindAll only if that fails
    // (classify_live then falls back to per-node Current* reads).
    let all = if let Some(c) = &cache {
        match root.FindAllBuildCache(TreeScope_Descendants, &true_cond, c) {
            Ok(a) => a,
            Err(e) => {
                tracing::debug!("tracker: FindAllBuildCache failed ({e}) — plain FindAll");
                match root.FindAll(TreeScope_Descendants, &true_cond) {
                    Ok(a) => a,
                    Err(_) => return (Vec::new(), 0, 0, 0, [0; 3]),
                }
            }
        }
    } else {
        match root.FindAll(TreeScope_Descendants, &true_cond) {
            Ok(a) => a,
            Err(_) => return (Vec::new(), 0, 0, 0, [0; 3]),
        }
    };
    let Ok(len) = all.Length() else { return (Vec::new(), 0, 0, 0, [0; 3]) };
    let mut ctx = WalkCtx {
        total: 0,
        docs: 0,
        docs_rects: Vec::new(),
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
    let nodes = ctx.total;
    let cands = ctx.candidates.len() as u32;
    let docs_n = ctx.docs_rects.len() as u32;
    let (specs, kinds) = ctx_to_specs(ctx, clip);
    (specs, nodes, cands, docs_n, kinds)
}

/// Visible-area clip for hole rects. `win` = foreground window rect,
/// physical px. `vis` = win ∩ virtual screen (all monitors) — None when
/// degenerate. Used by clamp_candidate to drop bogus UIA rects (see the
/// alpha.3/-4 field logs: a "player" image at (-218,-109), a stray
/// element at y=-11526) and to keep holes inside real on-screen area.
struct Clip {
    win: RECT,
    vis: Option<RECT>,
}

unsafe fn clip_rect(fg: HWND) -> Option<Clip> {
    let mut wr = RECT::default();
    GetWindowRect(fg, &mut wr).ok()?;
    if wr.right <= wr.left || wr.bottom <= wr.top {
        return None;
    }
    let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let vis = RECT {
        left: wr.left.max(vx),
        top: wr.top.max(vy),
        right: wr.right.min(vx + GetSystemMetrics(SM_CXVIRTUALSCREEN)),
        bottom: wr.bottom.min(vy + GetSystemMetrics(SM_CYVIRTUALSCREEN)),
    };
    let vis = if vis.right > vis.left && vis.bottom > vis.top {
        Some(vis)
    } else {
        None
    };
    Some(Clip { win: wr, vis })
}

/// Truncated foreground-window title for the summary log. Char-boundary
/// safe (multi-byte titles — see the trunc lesson in win_main).
unsafe fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
    let s = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
    s.chars().take(48).collect()
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
            let rect = el.CachedBoundingRectangle().unwrap_or_default();
            offer_candidate(classify(ct, &lct), &rect, ctx);
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
/// recursion). Document elements are harvested as scoping rects; every
/// other element goes through the media classifier.
fn classify_live(el: &IUIAutomationElement, ctx: &mut WalkCtx) {
    unsafe {
        ctx.total += 1;
        // Cached-first: FindAllBuildCache filled these client-side —
        // zero extra cross-process trips. Current* only when the cache
        // is missing (plain-FindAll path, or a provider that skipped
        // the property).
        let ct = el
            .CachedControlType()
            .or_else(|_| el.CurrentControlType())
            .unwrap_or(UIA_CONTROLTYPE_ID(0));
        let rect = el
            .CachedBoundingRectangle()
            .or_else(|_| el.CurrentBoundingRectangle())
            .unwrap_or_default();
        if ct == UIA_DocumentControlTypeId {
            // The page area (and iframes) — candidate scoping anchors.
            // Degenerate rects are skipped so a failed read can't turn
            // into a docs_rects that swallows every candidate.
            if rect.right > rect.left && rect.bottom > rect.top {
                ctx.docs_rects.push(rect);
            }
            return;
        }
        let lct = el
            .CachedLocalizedControlType()
            .or_else(|_| el.CurrentLocalizedControlType())
            .map(|s| s.to_string())
            .unwrap_or_default();
        offer_candidate(classify(ct, &lct), &rect, ctx);
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

/// Size gate + zero-rect rejection. Physical pixels. One gate for all
/// kinds (alpha.8): videos joined the 80px rule so hidden ad <video>
/// elements can never anchor a hole.
fn offer_candidate(kind: Option<MediaKind>, rect: &RECT, ctx: &mut WalkCtx) {
    let Some(kind) = kind else { return };
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    if w < MIN_MEDIA || h < MIN_MEDIA {
        return;
    }
    ctx.candidates.push(Candidate {
        x: rect.left,
        y: rect.top,
        w,
        h,
        min: MIN_MEDIA,
        kind,
    });
}

// =====================================================================
// Candidates → HoleSpecs (per-kind dominance + flatness + merge + cap)
// =====================================================================

fn ctx_to_specs(ctx: WalkCtx, clip: Option<Clip>) -> (Vec<HoleSpec>, [u32; 3]) {
    // Filter (bogus-rect + document-pane scoping) → clamp (final hole
    // geometry) → per-kind dominance → flatness → videos-first sort →
    // merge → cap.
    let WalkCtx { candidates, docs_rects, .. } = ctx;
    let mut cands: Vec<Candidate> = match clip {
        Some(clip) => candidates
            .into_iter()
            .filter_map(|c| clamp_candidate(c, &clip, &docs_rects))
            .collect(),
        None => candidates,
    };
    // Kind counts of the pool that entered selection — the summary
    // log's v=/i=/l= (a zero here means the walk never SAW the media;
    // a nonzero with holes=0 means selection dropped it).
    let mut kinds = [0u32; 3];
    for c in &cands {
        kinds[c.kind.slot()] += 1;
    }
    if cands.is_empty() {
        return (Vec::new(), kinds);
    }

    // PER-KIND DOMINANCE — keep, per kind, candidates within
    // KEEP_MIN_PCT% of the largest candidate OF THAT KIND. The player
    // and the sidebar thumbnails coexist (different kinds), while
    // avatar/icon swarms still die (tiny next to their kind's largest).
    let mut max_kind = [0i64; 3];
    for c in &cands {
        let a = c.area();
        if a > max_kind[c.kind.slot()] {
            max_kind[c.kind.slot()] = a;
        }
    }
    cands.retain(|c| c.area() * 100 >= max_kind[c.kind.slot()] * KEEP_MIN_PCT);

    // FLATNESS — image candidates only: a rect whose on-screen pixels
    // are uniform (solid spacer, bogus scroll-offset rect parked on
    // flat page area) earns no hole. Videos are trusted — a paused
    // talking-head frame can legitimately be flat.
    // SAFETY: screen_rect_is_flat only reads the screen DC (GetPixel).
    cands.retain(|c| {
        c.is_video() || unsafe { !screen_rect_is_flat(c.x, c.y, c.w, c.h) }
    });
    if cands.is_empty() {
        return (Vec::new(), kinds);
    }

    // Videos first, then largest first — a capped page keeps its
    // player before its thumbnails.
    cands.sort_by_key(|c| {
        (std::cmp::Reverse(c.is_video()), std::cmp::Reverse(c.area()))
    });

    // Overlap merge (TIGHTENED) — an <img> inside an <a> is ONE
    // thumbnail (intersection ≥ 40% of the smaller), but rects that
    // merely share a corner or edge stay SEPARATE. The old any-touch
    // rule unioned candidates into giant misplaced holes.
    let mut kept: Vec<Candidate> = Vec::with_capacity(MAX_KEEP);
    for c in cands {
        if kept.len() >= MAX_KEEP {
            break;
        }
        let mut merged = false;
        for k in kept.iter_mut() {
            if overlap_ratio(&c, k) >= 0.4 {
                let nx = k.x.min(c.x);
                let ny = k.y.min(c.y);
                let nr = k.x.max(c.x + c.w);
                let nb = k.y.max(c.y + c.h);
                k.x = nx;
                k.y = ny;
                k.w = nr - nx;
                k.h = nb - ny;
                merged = true;
                break;
            }
        }
        if !merged {
            kept.push(c);
        }
    }

    let specs = kept
        .into_iter()
        .map(|k| HoleSpec {
            x: k.x,
            y: k.y,
            w: k.w,
            h: k.h,
            dynamic: k.is_video(),
            kind: k.kind_id(),
        })
        .collect();
    (specs, kinds)
}

/// True when the on-screen pixels under the rect look uniform (solid
/// background / spacer graphic / bogus rect parked on flat page area).
/// Samples a 5×5 grid via GetPixel on the screen DC — the desktop shows
/// the INVERTED content, but inversion preserves variance, so the
/// thresholds hold in every mode. Failure-conservative: if the screen
/// DC can't be read, the rect is KEPT (can't tell → don't drop).
unsafe fn screen_rect_is_flat(x: i32, y: i32, w: i32, h: i32) -> bool {
    use windows::Win32::Graphics::Gdi::{GetDC, GetPixel, ReleaseDC};
    let hdc = GetDC(None);
    if hdc.is_invalid() {
        return false;
    }
    let mut min_l = i32::MAX;
    let mut max_l = i32::MIN;
    let mut colors: Vec<u32> = Vec::new();
    let mut samples = 0u32;
    for i in 0..5u32 {
        for j in 0..5u32 {
            let px = x + w * (2 * i as i32 + 1) / 10;
            let py = y + h * (2 * j as i32 + 1) / 10;
            let c = GetPixel(hdc, px, py);
            if c.0 == 0xFFFF_FFFF {
                continue; // CLR_INVALID — outside the visible screen
            }
            let (r, g, b) = (
                (c.0 & 0xFF) as i32,
                ((c.0 >> 8) & 0xFF) as i32,
                ((c.0 >> 16) & 0xFF) as i32,
            );
            let lum = (r * 299 + g * 587 + b * 114) / 1000;
            min_l = min_l.min(lum);
            max_l = max_l.max(lum);
            // 3-bit-per-channel coarse quantization for a cheap
            // distinct-color count (photo ≥ 10, flat ≤ 4).
            let coarse = (((r >> 5) as u32) << 6) | (((g >> 5) as u32) << 3) | ((b >> 5) as u32);
            if !colors.contains(&coarse) {
                colors.push(coarse);
            }
            samples += 1;
        }
    }
    ReleaseDC(None, hdc);
    if samples < 10 {
        return false; // mostly unreadable — keep
    }
    (max_l - min_l) < 28 && colors.len() <= 4
}

/// Placement filter for one candidate. Three gates, in order:
///
/// 1. BOGUS-POKE GUARD — real on-page media lives INSIDE its window;
///    a rect poking more than POKE_TOLERANCE past any window edge is a
///    mangled/hidden UIA element (field evidence: "player" images at
///    (-218,-109) and y=-11526 that became giant misplaced holes).
///    Dropped, not clamped — a clamp would still paint a huge hole
///    over the wrong area.
/// 2. DOCUMENT-PANE SCOPING — when the walk saw any Document element
///    (browser page / iframes / editor content), a candidate's CENTER
///    must sit inside one of those rects, and that rect then becomes
///    the clamp boundary too (a hole can never poke out of its page).
///    Apps exposing no documents keep the old window-clip behavior.
/// 3. CLAMP — the survivors are clipped to the scoping chain (document
///    ∩ window ∩ virtual screen) and must still pass their own size
///    gate.
fn clamp_candidate(mut c: Candidate, clip: &Clip, docs: &[RECT]) -> Option<Candidate> {
    // 1) bogus-poke guard
    if c.x < clip.win.left - POKE_TOLERANCE
        || c.y < clip.win.top - POKE_TOLERANCE
        || c.x + c.w > clip.win.right + POKE_TOLERANCE
        || c.y + c.h > clip.win.bottom + POKE_TOLERANCE
    {
        return None;
    }
    // 2) document-pane scoping — center must sit inside a Document;
    //    that Document also becomes the clamp boundary (gate 3).
    let mut scope: Option<RECT> = None;
    if !docs.is_empty() {
        let (cx, cy) = (c.x + c.w / 2, c.y + c.h / 2);
        scope = docs
            .iter()
            .copied()
            .find(|d| cx >= d.left && cx < d.right && cy >= d.top && cy < d.bottom);
        scope?;
    }
    // 3) clamp to the scoping chain: doc (if any) ∩ window ∩ virtual screen
    let mut l = c.x.max(clip.win.left);
    let mut t = c.y.max(clip.win.top);
    let mut r = (c.x + c.w).min(clip.win.right);
    let mut b = (c.y + c.h).min(clip.win.bottom);
    if let Some(d) = scope {
        l = l.max(d.left);
        t = t.max(d.top);
        r = r.min(d.right);
        b = b.min(d.bottom);
    }
    let (l, t, r, b) = match clip.vis {
        Some(v) => (l.max(v.left), t.max(v.top), r.min(v.right), b.min(v.bottom)),
        None => (l, t, r, b),
    };
    let (w, h) = (r - l, b - t);
    if w < c.min || h < c.min {
        return None;
    }
    c.x = l;
    c.y = t;
    c.w = w;
    c.h = h;
    Some(c)
}
