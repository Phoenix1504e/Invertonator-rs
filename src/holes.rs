// Invertonator-rs — holes: the true-color hole pool + main-thread pump
//
//   Each hole = a layered topmost host window hosting a Magnifier-class
//   child. The child re-captures its source region and applies the
//   INVERT color effect — media under a hole passes through the OS
//   inversion twice and renders in TRUE colors.
//
//   REFRESH MODEL (alpha.9): the Magnifier child re-captures ONLY when
//   its paint is poked — InvalidateRect — which is what Microsoft's
//   windowed-magnifier sample does every ~16ms. alpha.6..8 poked DYNAMIC
//   holes every tick but static ones only every 5th (~6.6Hz): any video
//   fragment misclassified as image/graphic then updated at 6.6Hz —
//   video-in-hole read as LAG, exactly the alpha.8 complaint. alpha.9
//   pokes EVERY visible hole EVERY tick (~33Hz): the poke is just a
//   repaint flag, the capture cost scales with hole AREA (not window
//   count), and Microsoft's own sample pokes a fullscreen magnifier at
//   60Hz. Classification can no longer throttle live content.
//   MagSetWindowSource (expensive pipeline rebuild) is reserved for
//   rect changes, re-shows and the rare static self-heal.
//
//   EVIDENCE RULE (alpha.11, CONTEXT-AWARE since alpha.14): specs are
//   TRUSTED ONLY WHILE FRESH — but "fresh" now keys on CONTEXT. The
//   pump hides every hole whose spec set (a) has not been re-confirmed
//   by a completed walk within STALE_SPEC_AGE AND (b) describes a
//   foreground context that has since changed (fg hwnd + title hash,
//   see tracker::fg_identity). Same context = the screen still shows
//   what the specs describe — a chronically slow provider (YouTube
//   under load) may take seconds per walk, and that must not blank a
//   page that has not changed (the alpha.13 "no hole in youtube"
//   residual). Changed context (tab switch / alt-tab / retitle) hides
//   unconfirmed holes within 4s — the alpha.10 danger (stale holes
//   outliving their page) stays structurally impossible, but a hole
//   CAN now outlive the clock while the screen it describes is still
//   on screen.
//
//   REFRESH RATE (alpha.12): the pump runs at 15ms (≈64Hz with
//   timeBeginPeriod(1)) and hole hosts are NOT layered. The 30ms
//   pump relied on WM_TIMER without timer-resolution help — it
//   actually fired every 31–46ms on the 15.6ms scheduler grid, so
//   hole content refreshed at an effective 22–32Hz while the page
//   itself ran at 60fps ("video is a little slow in the hole"). The
//   opaque WS_EX_LAYERED host (alpha 255) additionally forced every
//   magnifier repaint through DWM redirection composition — pure
//   overhead the proven F11 probe host never had. Both gone.
//
//   THREADING MODEL (v0.2.0-alpha.3 — the "F11 rule"):
//   · Hole windows are created on the MAIN thread and stay there.
//     Windows cannot move between threads: the OS ties every HWND to
//     its creating thread, and the "Magnifier" class runs its capture
//     on WM_TIMER inside its owning thread's message queue. The old
//     pump-thread design called MagSetWindowSource/MoveWindow/
//     ShowWindow cross-thread — that path was NEVER proven (the F11
//     probe proved main-thread calls only) and is the prime suspect
//     for dead/stale holes.
//   · So: the pump is a SetTimer(30ms) on main. Every tick,
//     HolePool::pump_tick() reconciles the shared desired-state
//     (specs/active/pierce/hold) onto the windows — same thread that
//     created them, same thread that pumps their messages. All Mag*
//     and window calls are on the owning thread, exactly like the
//     proven F11 probe.
//   · Desired state is written cross-thread by the tracker (specs)
//     and the UI thread (active/pierce/hold) through atomics + a
//     mutex — the pump remains the SINGLE WRITER of window state.
//
//   LEDGER LESSONS BAKED IN:
//   · MagSetColorEffect — the CORRECT name (linker caught the
//     hallucinated MagSetWindowColorEffect in the F11 probe)
//   · RAW CreateWindowExW + null-check (the crate's Result wrapper
//     produces "failed successfully" false errors)

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, MoveWindow, RegisterClassW, ShowWindow, SHOW_WINDOW_CMD,
    WNDCLASSW,
};

// ---- Magnification window-level externs (proven via F11 probe) ----

#[link(name = "Magnification")]
extern "system" {
    fn MagSetColorEffect(hwnd: HWND, matrix: *const f32) -> BOOL;
    fn MagSetWindowSource(hwnd: HWND, rect: RECT) -> BOOL;
    fn MagSetWindowTransform(hwnd: HWND, matrix: *const f32) -> BOOL;
}

// ---- RAW CreateWindowExW (module-scoped — main has its own) ----

#[link(name = "user32")]
extern "system" {
    fn CreateWindowExW(
        dwexstyle: u32,
        lpclassname: *const u16,
        lpwindowname: *const u16,
        dwstyle: u32,
        x: i32,
        y: i32,
        nwidth: i32,
        nheight: i32,
        hwndparent: HWND,
        hmenu: isize,
        hinstance: HINSTANCE,
        lpparam: *mut std::ffi::c_void,
    ) -> HWND;
}

const INVERT_FLAT: [f32; 25] = [
    -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0,
];

const IDENTITY_9: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

const MAGNIFIER_CLASS: PCWSTR = w!("Magnifier");
const HOST_CLASS: PCWSTR = w!("InvertonatorHoleHost");

const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_HIDE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(0);
const SW_SHOWNOACTIVATE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(4);
pub const PUMP_MS: u32 = 15; // pump cadence (SetTimer, ms) — with
                             // timeBeginPeriod(1) this lands at ~64Hz;
                             // without it a 30ms timer drifted on the
                             // 15.6ms scheduler grid and actually fired
                             // every 31–46ms (~22–32Hz, wobbling) — the
                             // "video is a little slow in the hole"
                             // report. Same rate as Microsoft's own
                             // windowed-magnifier sample (~16ms).
const STATIC_EVERY: u64 = 1200; // static-hole self-heal, in ticks
                                // (1200 × 15ms ≈ 18s — kept at the
                                // alpha.9 interval when the pump doubled;
                                // a pipeline reset on a misclassified
                                // video hole is a visible stutter)
pub const MAX_HOLES: usize = 16;
// EVIDENCE TTL (alpha.11 → CONTEXT-AWARE in alpha.14): how long a
// published hole set stays trusted without a fresh confirming walk —
// but ONLY when the foreground context changed since the publish (see
// the pump). Normal publish gaps are ≤1.5s (backoff cap) + walk time;
// a wedged provider (tab teardown is the classic case) stops
// publishes entirely, and the pump would otherwise keep showing the
// last spec set FOREVER — the "holes are permanent even after shifting
// tabs" report. alpha.14 splits the outcomes:
//   · context UNCHANGED (slow-but-alive provider, video still there):
//     holes STAY — a slow walk must not blank an unchanged page (the
//     alpha.13 "no hole in youtube" residual);
//   · context CHANGED (tab switch, alt-tab, retitle): unconfirmed
//     holes hide within 4s — half the old 8s, because the
//     same-context grace no longer has to be priced into the window.
const STALE_SPEC_AGE: Duration = Duration::from_millis(4000);

// CAPTURE MODEL (alpha.6 — measured, not assumed):
// the "Magnifier" control does NOT stream its source region on its
// own. It re-captures only when its paint is poked. Microsoft's own
// windowed-magnifier sample drives it with a ~16ms SetTimer whose
// callback is just SetWindowPos(topmost) + InvalidateRect(hwndMag).
// alpha.5 trusted the control to live-update after a single
// MagSetWindowSource and only re-armed on rect change — the holes
// froze on their first captured frame ("nope not working").
//
// So the pump pokes EVERY visible hole with InvalidateRect EVERY tick
// (~33Hz) — no dynamic/static split. That one rule fixes:
//   · "video in the hole is frozen" (alpha.5 — no poke at all)
//   · "video in the hole is very laggy" (alpha.4 — reset spam)
//   · "video is still laggy" (alpha.8 — a video fragment classified
//     img/graphic was a STATIC hole: 6.6Hz poke + a ~6s pipeline-reset
//     self-heal; live content behind a static-classified hole)
// InvalidateRect is CHEAP (just a repaint flag — no pipeline reset).
// MagSetWindowSource — the expensive call that tears down and rebuilds
// the DWM frame source — stays reserved for what only it can do:
// pointing the hole at a new rect (move / re-show / rare self-heal).

// =====================================================================
// Public API
// =====================================================================

/// One desired hole. Coordinates are SCREEN physical pixels (the
/// process is per-monitor-v2 DPI aware — see main::set_dpi_awareness).
#[derive(Clone, Copy, Default, Debug)]
pub struct HoleSpec {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// true = video-adjacent: discovery walks stay at the fast cadence
    /// while it exists (tracker-side) and it never self-heals. Content
    /// refresh no longer depends on it — alpha.9 pokes every hole at
    /// ~33Hz regardless.
    pub dynamic: bool,
}

/// Clone-able handle to the hole system's desired-state. The tracker
/// thread and UI thread both hold one; the actual windows live in the
/// HolePool owned by main.
#[derive(Clone)]
pub struct Holes {
    pub(crate) shared: Arc<HolesShared>,
}

pub struct HolesShared {
    // The desired hole set — written by the tracker, read by the pump.
    pub specs: Mutex<Vec<HoleSpec>>,
    /// Wall-clock stamp of the last publish (alpha.11). set_specs is
    /// called exactly once per COMPLETED walk — empty results count —
    /// so this is the pump's independent evidence clock: it advances
    /// only while the tracker thread is alive AND its walks complete.
    pub published_at: Mutex<Instant>,
    /// alpha.14 — the foreground CONTEXT (hwnd, fnv1a(title)) the
    /// published specs describe, stamped by set_specs_ctx. The pump
    /// samples the CURRENT context at ~2Hz; equality = "the screen
    /// still shows what the specs describe" (see pump_tick —
    /// CONTEXT-AWARE EVIDENCE). (0,0) = contextless (clear paths).
    pub published_fg: Mutex<(usize, u64)>,
    pub active: AtomicBool, // false = holes hidden (Dim/Off)
    pub pierce: AtomicBool, // manual hide-all (F7)
    pub hold: AtomicBool,   // scroll-hold (holes hidden during flicks)
}

impl Holes {
    /// Creates the pool (MAX_HOLES host+magnifier window pairs, parked
    /// offscreen and hidden). MUST be called from the main thread — the
    /// same thread that runs the message loop pumping these windows.
    /// Returns the shared-state handle and the window pool; keep both.
    pub fn new() -> (Self, HolePool) {
        let pool = unsafe { create_pool() };
        tracing::info!("holes: pool created ({} windows)", pool.pool.len());

        let shared = Arc::new(HolesShared {
            specs: Mutex::new(Vec::new()),
            published_at: Mutex::new(Instant::now()),
            published_fg: Mutex::new((0, 0)),
            active: AtomicBool::new(true),
            pierce: AtomicBool::new(false),
            hold: AtomicBool::new(false),
        });

        (Self { shared }, pool)
    }

    /// Replace the desired hole set WITHOUT context (clear paths —
    /// the specs are empty, so the evidence context is moot).
    pub fn set_specs(&self, specs: Vec<HoleSpec>) {
        self.set_specs_ctx(specs, (0, 0));
    }

    /// Replace the desired hole set (screen physical px) STAMPED with
    /// the foreground context it describes (tracker::fg_identity).
    /// Call at discovery cadence. Surplus holes are hidden
    /// automatically. ALSO re-stamps the evidence clock — every
    /// completed walk (even one that found nothing) is fresh evidence
    /// that the tracker is alive and its view of the screen is current.
    pub fn set_specs_ctx(&self, specs: Vec<HoleSpec>, fg_ctx: (usize, u64)) {
        *self.shared.specs.lock().unwrap() = specs;
        *self.shared.published_at.lock().unwrap() = Instant::now();
        *self.shared.published_fg.lock().unwrap() = fg_ctx;
    }

    /// Scroll-hold: holes vanish while set (manual API; the automatic
    /// path is scrollhook::hold_active — see pump_tick).
    #[allow(dead_code)]
    pub fn set_hold(&self, on: bool) {
        self.shared.hold.store(on, Ordering::Relaxed);
    }

    /// Master switch: false hides all holes (Dim/Off modes).
    pub fn set_active(&self, on: bool) {
        self.shared.active.store(on, Ordering::Relaxed);
    }

    /// Pierce: manually hide all holes (F7-style toggle).
    pub fn set_pierce(&self, on: bool) {
        self.shared.pierce.store(on, Ordering::Relaxed);
    }
}

/// The hole windows themselves. MAIN-THREAD ONLY — deliberately not
/// Send/Sync. `pump_tick` runs from WM_TIMER on main.
pub struct HolePool {
    pool: Vec<PoolHole>,
    tick: u64,
    /// alpha.11 — logs the stale-latch and recovery transitions once
    /// each instead of every 30ms tick.
    stale_latched: bool,
    /// alpha.14 — the CURRENT foreground context, sampled at ~2Hz
    /// (the identity read is one GetWindowTextW on a cached caption;
    /// sampling at the full 64Hz would be 60+ pointless reads/s).
    cur_fg: (usize, u64),
    /// alpha.12 — pump-rate heartbeat window (actual delivery Hz is
    /// the number that decides whether video in the hole looks
    /// smooth; a sagging rate = the main thread is starved).
    beat_tick: u64,
    beat_at: std::time::Instant,
}

impl HolePool {
    /// One reconciliation pass: apply the shared desired state to the
    /// windows. Runs on main at PUMP_MS cadence (SetTimer → WM_TIMER).
    pub fn pump_tick(&mut self, shared: &HolesShared) {
        self.tick += 1;
        let tick = self.tick;

        // EVIDENCE (alpha.11 → CONTEXT-AWARE in alpha.14): the pump
        // never trusts a spec set older than STALE_SPEC_AGE — unless
        // the foreground context still matches the one the specs
        // describe. This is deliberately judged HERE, on the main
        // thread, from its own wall clock — independent of the tracker
        // thread's health:
        //   · fresh publish → holes on (unchanged from alpha.11);
        //   · stale publish + SAME context → holes STAY: a
        //     slow-but-alive provider (YouTube under load) or even a
        //     wedged walk cannot blank a page that has not changed —
        //     the alpha.13 "no hole in youtube" residual. The video
        //     the specs describe is provably still the thing on
        //     screen (same hwnd, same title);
        //   · stale publish + CHANGED context (tab switch, alt-tab,
        //     retitle) → holes hide — the alpha.10 stale-holes danger
        //     (old page's holes outliving their page) stays covered,
        //     now within 4s instead of 8s.
        if tick.is_multiple_of(32) {
            self.cur_fg = crate::tracker::fg_identity_current();
        }
        let age = shared.published_at.lock().unwrap().elapsed();
        let fresh = age < STALE_SPEC_AGE;
        let same_context = {
            let pub_fg = *shared.published_fg.lock().unwrap();
            pub_fg != (0usize, 0u64) && pub_fg == self.cur_fg
        };
        let evidence_ok = fresh || same_context;
        if !evidence_ok && !self.stale_latched {
            self.stale_latched = true;
            tracing::warn!(
                "holes: spec evidence stale ({:.1}s) AND fg context changed — hiding all holes until a fresh walk re-confirms the current page",
                age.as_secs_f32()
            );
        } else if evidence_ok && self.stale_latched {
            self.stale_latched = false;
            tracing::info!(
                "holes: spec evidence ok (fresh walk or unchanged context) — holes re-enabled"
            );
        }

        // want_visible — the pump is the SINGLE writer of hole visibility.
        // Scroll-hold fires from the WH_MOUSE_LL hook (see scrollhook).
        let want_visible = shared.active.load(Ordering::Relaxed)
            && !shared.pierce.load(Ordering::Relaxed)
            && !shared.hold.load(Ordering::Relaxed)
            && !crate::scrollhook::hold_active()
            && evidence_ok;

        let specs = shared.specs.lock().unwrap().clone();

        for (i, spec) in specs.iter().enumerate() {
            if i >= self.pool.len() {
                break;
            }
            let h = &mut self.pool[i];

            let src = RECT {
                left: spec.x,
                top: spec.y,
                right: spec.x + spec.w,
                bottom: spec.y + spec.h,
            };
            let moved = h.source.left != src.left
                || h.source.top != src.top
                || h.source.right != src.right
                || h.source.bottom != src.bottom;
            h.dynamic = spec.dynamic;

            if moved {
                h.source = src;
                unsafe {
                    let _ = MoveWindow(h.host, spec.x, spec.y, spec.w, spec.h, true);
                    let _ = MoveWindow(h.mag, 0, 0, spec.w, spec.h, true);
                    let _ = MagSetWindowSource(h.mag, src);
                }
            }

            unsafe {
                // Visibility reconcile — the pump is the single writer.
                if want_visible && !h.visible {
                    let _ = ShowWindow(h.host, SW_SHOWNOACTIVATE_CMD);
                    h.visible = true;
                    // Re-arm the capture on (re)show: the Magnifier drops
                    // its frame pool while hidden, and an unchanged rect
                    // never reaches the moved-branch to restore it.
                    let _ = MagSetWindowSource(h.mag, h.source);
                } else if !want_visible && h.visible {
                    let _ = ShowWindow(h.host, SW_HIDE_CMD);
                    h.visible = false;
                }

                // LIVE-CONTENT POKE (alpha.9 — every hole, every tick).
                // The Magnifier control re-captures ONLY when repainted
                // (see the capture-model note above). Without a poke a
                // hole shows one static frame forever — the alpha.5
                // regression; without EVERY-tick pokes a static-
                // classified hole showing live content updates at 6.6Hz
                // — the alpha.8 "video is still laggy" regression.
                if want_visible && h.visible {
                    let _ = InvalidateRect(h.mag, None, false);
                }

                // Static-hole self-heal (~18s): a rare MagSetWindowSource
                // re-arm in case a display-mode change froze the frame
                // source entirely. Video holes never get one — their
                // content is kept live by the poke above, and a source
                // reset mid-playback is exactly the alpha.4 stutter.
                if want_visible && h.visible && !h.dynamic && tick.is_multiple_of(STATIC_EVERY) {
                    let _ = MagSetWindowSource(h.mag, h.source);
                }
            }
        }

        // Surplus holes: hide.
        for h in self.pool.iter_mut().skip(specs.len()) {
            if h.visible {
                unsafe {
                    let _ = ShowWindow(h.host, SW_HIDE_CMD);
                }
                h.visible = false;
            }
        }

        // PUMP-RATE HEARTBEAT (alpha.12): once per 10s while holes
        // should be showing, log the ACTUAL tick rate. The design rate
        // is ~64Hz; a sagging number means the main thread is starved
        // (timer resolution lost, system load) — the measurable form
        // of "video looks a little slow in the hole".
        if want_visible && self.beat_at.elapsed() >= Duration::from_secs(10) {
            let dt = self.beat_at.elapsed().as_secs_f64();
            let ticks = self.tick - self.beat_tick;
            let vis = self.pool.iter().filter(|h| h.visible).count();
            tracing::info!(
                "holes: pump {:.1}Hz ticks={} holes={vis}",
                ticks as f64 / dt,
                ticks
            );
            self.beat_tick = self.tick;
            self.beat_at = Instant::now();
        }
    }
}

// =====================================================================
// Pool internals — main thread only.
// =====================================================================

struct PoolHole {
    host: HWND,
    mag: HWND,
    // pump-local last-applied state (single writer: the main-thread pump)
    source: RECT,
    visible: bool,
    dynamic: bool,
}

unsafe extern "system" fn hole_host_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // WM_NCHITTEST → HTTRANSPARENT: clicks pass through to the window
    // beneath (belt-and-suspenders — WS_EX_TRANSPARENT already covers it).
    const WM_NCHITTEST: u32 = 0x0084;
    if msg == WM_NCHITTEST {
        return LRESULT(-1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Creates the MAX_HOLES host+magnifier pairs. MAIN THREAD ONLY.
unsafe fn create_pool() -> HolePool {
    let hinstance = HINSTANCE::from(GetModuleHandleW(None).unwrap());

    // Host window class — click-through via WM_NCHITTEST.
    let wc = WNDCLASSW {
        lpfnWndProc: Some(hole_host_wndproc),
        hInstance: hinstance,
        lpszClassName: HOST_CLASS,
        ..Default::default()
    };
    // "already registered" is fine — idempotent per process.
    let _ = RegisterClassW(&wc);

    let mut pool: Vec<PoolHole> = Vec::with_capacity(MAX_HOLES);
    for _ in 0..MAX_HOLES {
        // WS_EX_TOPMOST: holes live in the topmost band FOREVER — the
        // foreground app (browser playing video) can never cover them,
        // no matter how often the user clicks around.
        // NOT layered (alpha.12): an opaque layered host forces every
        // magnifier repaint through DWM redirection composition — pure
        // overhead. The proven F11 probe host was never layered.
        let host = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            HOST_CLASS.as_ptr(),
            w!("Invertonator hole").as_ptr(),
            WS_POPUP | WS_DISABLED, // disabled → skipped in hit-testing
            -10000,
            -10000,
            8,
            8, // parked offscreen
            HWND::default(),
            0,
            hinstance,
            std::ptr::null_mut(),
        );
        if host.is_invalid() {
            tracing::error!(
                "holes: host creation failed — pool truncated at {}",
                pool.len()
            );
            break;
        }

        let mag = CreateWindowExW(
            0,
            MAGNIFIER_CLASS.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            8,
            8,
            host,
            0,
            hinstance,
            std::ptr::null_mut(),
        );
        if mag.is_invalid() {
            tracing::error!(
                "holes: Magnifier child failed — pool truncated at {}",
                pool.len()
            );
            let _ = DestroyWindow(host);
            break;
        }

        // Per-window color effect: INVERT (the double-invert trick).
        let _ = MagSetColorEffect(mag, INVERT_FLAT.as_ptr());
        // 1:1 transform — show the source region unscaled.
        let _ = MagSetWindowTransform(mag, IDENTITY_9.as_ptr());

        pool.push(PoolHole {
            host,
            mag,
            source: RECT::default(),
            visible: false,
            dynamic: false,
        });
    }

    HolePool {
        pool,
        tick: 0,
        stale_latched: false,
        cur_fg: (0, 0),
        beat_tick: 0,
        beat_at: Instant::now(),
    }
}
