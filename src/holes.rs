// Invertonator-rs — holes: the true-color hole pool + main-thread pump
//
//   Each hole = a layered topmost host window hosting a Magnifier-class
//   child. The child re-captures its source region and applies the
//   INVERT color effect — media under a hole passes through the OS
//   inversion twice and renders in TRUE colors.
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

use windows::Win32::Foundation::{
    BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, MoveWindow, RegisterClassW,
    SetLayeredWindowAttributes, ShowWindow,
    LAYERED_WINDOW_ATTRIBUTES_FLAGS, SHOW_WINDOW_CMD, WNDCLASSW,
};
use windows::core::{w, PCWSTR};

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
        x: i32, y: i32, nwidth: i32, nheight: i32,
        hwndparent: HWND,
        hmenu: isize,
        hinstance: HINSTANCE,
        lpparam: *mut std::ffi::c_void,
    ) -> HWND;
}

const INVERT_FLAT: [f32; 25] = [
    -1.0, 0.0, 0.0, 0.0, 0.0,
    0.0, -1.0, 0.0, 0.0, 0.0,
    0.0, 0.0, -1.0, 0.0, 0.0,
    0.0, 0.0, 0.0, 1.0, 0.0,
    1.0, 1.0, 1.0, 0.0, 1.0,
];

const IDENTITY_9: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

const MAGNIFIER_CLASS: PCWSTR = w!("Magnifier");
const HOST_CLASS: PCWSTR = w!("InvertonatorHoleHost");

const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_DISABLED: u32 = 0x0800_0000;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const LWA_ALPHA_FLAG: LAYERED_WINDOW_ATTRIBUTES_FLAGS =
    LAYERED_WINDOW_ATTRIBUTES_FLAGS(2);
const SW_HIDE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(0);
const SW_SHOWNOACTIVATE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(4);
pub const PUMP_MS: u32 = 30;      // pump cadence (SetTimer, ms)
const STATIC_EVERY: u64 = 200;    // static-hole self-heal ~0.17Hz
pub const MAX_HOLES: usize = 16;

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
    /// true = video-adjacent: capture is (re-)armed ONLY on a real rect
    /// change or a re-show. false = additionally self-heals ~0.17Hz.
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
    pub active: AtomicBool,   // false = holes hidden (Dim/Off)
    pub pierce: AtomicBool,   // manual hide-all (F7)
    pub hold: AtomicBool,     // scroll-hold (holes hidden during flicks)
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
            active: AtomicBool::new(true),
            pierce: AtomicBool::new(false),
            hold: AtomicBool::new(false),
        });

        (Self { shared }, pool)
    }

    /// Replace the desired hole set (screen physical px). Call at
    /// discovery cadence. Surplus holes are hidden automatically.
    pub fn set_specs(&self, specs: Vec<HoleSpec>) {
        *self.shared.specs.lock().unwrap() = specs;
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
}

impl HolePool {
    /// One reconciliation pass: apply the shared desired state to the
    /// windows. Runs on main at PUMP_MS cadence (SetTimer → WM_TIMER).
    pub fn pump_tick(&mut self, shared: &HolesShared) {
        self.tick += 1;
        let tick = self.tick;

        // want_visible — the pump is the SINGLE writer of hole visibility.
        // Scroll-hold fires from the WH_MOUSE_LL hook (see scrollhook).
        let want_visible = shared.active.load(Ordering::Relaxed)
            && !shared.pierce.load(Ordering::Relaxed)
            && !shared.hold.load(Ordering::Relaxed)
            && !crate::scrollhook::hold_active();

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

                // CAPTURE DISCIPLINE (alpha.5 — the lag fix):
                // every MagSetWindowSource restarts the Magnifier's
                // capture pipeline. alpha.4 re-armed dynamic (video)
                // holes EVERY 30ms tick — 33 pipeline resets per second
                // with the rect unchanged — and the video inside the
                // hole stuttered. Now: re-arm only on a real rect change
                // (above) or a re-show (above). Static holes keep a slow
                // ~0.17Hz self-heal against a frozen capture after
                // display-mode changes; video holes never get one —
                // any content change that matters comes with a rect
                // change, and the Magnifier updates live otherwise.
                if want_visible
                    && h.visible
                    && !h.dynamic
                    && tick.is_multiple_of(STATIC_EVERY)
                {
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
        let host = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT
                | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            HOST_CLASS.as_ptr(),
            w!("Invertonator hole").as_ptr(),
            WS_POPUP | WS_DISABLED,   // disabled → skipped in hit-testing
            -10000, -10000, 8, 8,     // parked offscreen
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
        let _ = SetLayeredWindowAttributes(
            host,
            COLORREF(0),
            255,
            LWA_ALPHA_FLAG,
        );

        let mag = CreateWindowExW(
            0,
            MAGNIFIER_CLASS.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_VISIBLE,
            0, 0, 8, 8,
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

    HolePool { pool, tick: 0 }
}
