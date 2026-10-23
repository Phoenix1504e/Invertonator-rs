// Invertonator-rs — tracker: media classification gates → live holes
//
//   Runs a discovery loop on its own thread: every DISCOVERY_MS it
//   walks the FOREGROUND window's UIA tree (cached walk) and turns
//   media elements into HoleSpecs for the hole pump.
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
//   · Size gates: image/graphic ≥ 40×40 px (kills favicons, inline
//     icons), hyperlink ≥ 96×96 px (thumbnail-sized links like the
//     YouTube grid; text links stay inverted), video any size.
//   · Overlap merge: an <img> inside an <a> produces two candidates
//     for the same thumbnail — merged into one hole, biggest wins.
//   · Cap at MAX_HOLES, largest first.
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

use crate::holes::{HoleSpec, Holes, MAX_HOLES};

const DISCOVERY_MS: u64 = 400;
const MIN_MEDIA: i32 = 40;   // image/graphic min w and h (px)
const MIN_LINK: i32 = 96;    // hyperlink min w and h (px)
const MAX_NODES: u32 = 100_000;
const MAX_DEPTH: u32 = 25;
const POKE_TOLERANCE: i32 = 8;   // pt beyond the window edge → bogus rect
const SUMMARY_EVERY_MS: u64 = 2_000;  // state-change summary throttle
const HEARTBEAT_MS: u64 = 10_000;     // unconditional liveness line

/// One-shot flag: the Gecko live-fallback notice is logged exactly once
/// per process (discovery ticks every 400ms — per-tick logs would spam
/// log.txt at 150 lines/minute).
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
    tracing::info!("tracker: alive (discovery every {DISCOVERY_MS}ms)");

    let mut had_specs = false;
    let mut last_log = std::time::Instant::now();
    let mut last_key = String::new();
    while !stop.load(Ordering::Relaxed) {
        if enabled.load(Ordering::Relaxed) {
            let fg = unsafe { GetForegroundWindow() };
            let (specs, nodes, cands, docs_n) = if fg.is_invalid() {
                (Vec::new(), 0, 0, 0)
            } else {
                unsafe { discover(&automation, fg) }
            };
            had_specs = !specs.is_empty();
            holes.set_specs(specs.clone());

            // Throttled discovery summary — the alpha.3 log proved the
            // PROBE sees media but said nothing about what the TRACKER
            // produced. This line closes that observability gap: one
            // line per state change, plus a 10s heartbeat.
            let now = std::time::Instant::now();
            let elapsed = now.duration_since(last_log).as_millis() as u64;
            let title = if fg.is_invalid() {
                String::new()
            } else {
                // SAFETY: fg valid, plain HWND read (GetWindowTextW).
                unsafe { window_title(fg) }
            };
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
                    "tracker: fg=\"{}\" nodes={} cands={} docs={} holes={} [{}]",
                    title,
                    nodes,
                    cands,
                    docs_n,
                    specs.len(),
                    rects.join(" ")
                );
            }
        } else if had_specs {
            // Disabled while holes were out — clear once.
            holes.set_specs(Vec::new());
            had_specs = false;
        }
        std::thread::sleep(Duration::from_millis(DISCOVERY_MS));
    }
    tracing::info!("tracker: stopped");
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

struct Candidate {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// Size gate this candidate must keep passing after on-screen
    /// clamping (MIN_MEDIA / MIN_LINK / 1 for video).
    min: i32,
    dynamic: bool,
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
unsafe fn discover(automation: &IUIAutomation, fg: HWND) -> (Vec<HoleSpec>, u32, u32, u32) {
    let Ok(root) = automation.ElementFromHandle(fg) else {
        return (Vec::new(), 0, 0, 0);
    };
    // Hole rects are clamped to the foreground window ∩ virtual screen
    // (see clip_rect). Computed once per discovery tick.
    let clip = clip_rect(fg);

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
                let specs = ctx_to_specs(ctx, clip);
                return (specs, nodes, cands, docs_n);
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

    // FALLBACK: flat live FindAll — no tree recursion possible, but
    // Document elements harvested during the walk give the same
    // document-pane scoping the cached path had (see classify_live +
    // clamp_candidate), and the size gates still apply.
    tracing::debug!("tracker: cached walk unavailable — live fallback");
    let Ok(true_cond) = automation.CreateTrueCondition() else {
        return (Vec::new(), 0, 0, 0);
    };
    let Ok(all) = root.FindAll(TreeScope_Descendants, &true_cond) else {
        return (Vec::new(), 0, 0, 0);
    };
    let Ok(len) = all.Length() else { return (Vec::new(), 0, 0, 0) };
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
    (ctx_to_specs(ctx, clip), nodes, cands, docs_n)
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
        let ct = el.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
        let rect = el.CurrentBoundingRectangle().unwrap_or_default();
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
            .CurrentLocalizedControlType()
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

/// Size gates + zero-rect rejection. Physical pixels.
fn offer_candidate(kind: Option<MediaKind>, rect: &RECT, ctx: &mut WalkCtx) {
    let Some(kind) = kind else { return };
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;
    if w <= 0 || h <= 0 {
        return;
    }
    let (passes, min) = match kind {
        MediaKind::Image => (w >= MIN_MEDIA && h >= MIN_MEDIA, MIN_MEDIA),
        MediaKind::Link => (w >= MIN_LINK && h >= MIN_LINK, MIN_LINK),
        MediaKind::Video => (true, 1),
    };
    if !passes {
        return;
    }
    ctx.candidates.push(Candidate {
        x: rect.left,
        y: rect.top,
        w,
        h,
        min,
        dynamic: kind == MediaKind::Video,
    });
}

// =====================================================================
// Candidates → HoleSpecs (merge + cap)
// =====================================================================

impl Candidate {
    fn intersects(&self, ox: i32, oy: i32, ow: i32, oh: i32) -> bool {
        self.x < ox + ow && ox < self.x + self.w && self.y < oy + oh && oy < self.y + self.h
    }
}

fn ctx_to_specs(ctx: WalkCtx, clip: Option<Clip>) -> Vec<HoleSpec> {
    // Filter (bogus-rect + document-pane scoping) → clamp (final hole
    // geometry) → merge → cap.
    let WalkCtx { candidates, docs_rects, .. } = ctx;
    let cands = match clip {
        Some(clip) => candidates
            .into_iter()
            .filter_map(|c| clamp_candidate(c, &clip, &docs_rects))
            .collect(),
        None => candidates,
    };
    if cands.is_empty() {
        return Vec::new();
    }

    // Largest first.
    let mut cands = cands;
    cands.sort_by_key(|c| std::cmp::Reverse(c.w as i64 * c.h as i64));

    // Overlap merge: an <img> inside an <a> is ONE thumbnail. A new
    // candidate intersecting a kept one grows that hole to the union.
    let mut kept: Vec<Candidate> = Vec::with_capacity(MAX_HOLES);
    for c in cands {
        if kept.len() >= MAX_HOLES {
            break;
        }
        let mut merged = false;
        for k in kept.iter_mut() {
            if c.intersects(k.x, k.y, k.w, k.h) {
                let nx = k.x.min(c.x);
                let ny = k.y.min(c.y);
                let nr = k.x.max(c.x + c.w);
                let nb = k.y.max(c.y + c.h);
                k.x = nx;
                k.y = ny;
                k.w = nr - nx;
                k.h = nb - ny;
                k.dynamic = k.dynamic || c.dynamic;
                merged = true;
                break;
            }
        }
        if !merged {
            kept.push(c);
        }
    }

    kept.into_iter()
        .map(|k| HoleSpec { x: k.x, y: k.y, w: k.w, h: k.h, dynamic: k.dynamic })
        .collect()
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
///    must sit inside one of those rects. Kills holes on browser
///    chrome, toolbars and page furniture in the flat live walk.
///    Apps exposing no documents keep the old keep-all behavior.
/// 3. CLAMP — the survivors are clipped to the window ∩ virtual
///    screen (tiny pokes and monitor edges) and must still pass their
///    own size gate.
fn clamp_candidate(mut c: Candidate, clip: &Clip, docs: &[RECT]) -> Option<Candidate> {
    // 1) bogus-poke guard
    if c.x < clip.win.left - POKE_TOLERANCE
        || c.y < clip.win.top - POKE_TOLERANCE
        || c.x + c.w > clip.win.right + POKE_TOLERANCE
        || c.y + c.h > clip.win.bottom + POKE_TOLERANCE
    {
        return None;
    }
    // 2) document-pane scoping (center-in-document)
    if !docs.is_empty() {
        let (cx, cy) = (c.x + c.w / 2, c.y + c.h / 2);
        let inside = docs
            .iter()
            .any(|d| cx >= d.left && cx < d.right && cy >= d.top && cy < d.bottom);
        if !inside {
            return None;
        }
    }
    // 3) clamp to window, then to the virtual screen
    let l = c.x.max(clip.win.left);
    let t = c.y.max(clip.win.top);
    let r = (c.x + c.w).min(clip.win.right);
    let b = (c.y + c.h).min(clip.win.bottom);
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
