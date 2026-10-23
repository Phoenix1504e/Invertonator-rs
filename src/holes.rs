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
//   · v0.3.0-alpha.4 — HOSTS MUST BE WS_EX_TOPMOST. Non-topmost hosts
//     live in the normal Z band: the moment the user activates the
//     browser, the app raises ABOVE the hole windows and the pump
//     never re-raises them (ShowWindow only fires on the hidden→shown
//     transition). Result: holes vanish the first time anything gets
//     focused — exactly the "hole appeared then disappeared" report.
//     Fix: show via SetWindowPos(HWND_TOPMOST, SWP_SHOWWINDOW |
//     SWP_NOACTIVATE) and re-assert topmost on the static refresh
//     cadence. Click-through is unaffected (WS_EX_TRANSPARENT +
//     WM_NCHITTEST→HTTRANSPARENT still pass input through).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{
    BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DestroyWindow, MoveWindow, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, HWND_TOPMOST,
    LAYERED_WINDOW_ATTRIBUTES_FLAGS, SHOW_WINDOW_CMD, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, WNDCLASSW,
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
const WS_EX_TOPMOST: u32 = 0x0000_0001;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const LWA_ALPHA_FLAG: LAYERED_WINDOW_ATTRIBUTES_FLAGS =
    LAYERED_WINDOW_ATTRIBUTES_FLAGS(2);
const SW_HIDE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(0);
pub const PUMP_MS: u32 = 30;      // pump cadence (SetTimer, ms)
const STATIC_EVERY: u64 = 20;     // static holes refresh ~1.7Hz
pub const MAX_HOLES: usize = 16;

// =====================================================================
// Public API
// =====================================================================

// Hole kind tags — logging/diagnosis only, the pump geometry ignores
// them. 'V' = UIA video element, 'I' = image/graphic, 'L' = hyperlink.
pub const KIND_IMG: u8 = 0;
pub const KIND_LINK: u8 = 1;
pub const KIND_VIDEO: u8 = 2;

/// One desired hole. Coordinates are SCREEN physical pixels (the
/// process is per-monitor-v2 DPI aware — see win_main::set_dpi_awareness).
#[derive(Clone, Copy, Default, Debug)]
pub struct HoleSpec {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// true = refresh every pump tick (video-adjacent). false = ~1.7Hz.
    pub dynamic: bool,
    /// KIND_IMG / KIND_LINK / KIND_VIDEO — shows up in hole logs so a
    /// field log alone answers "what did the tracker think this was?".
    pub kind: u8,
}

impl HoleSpec {
    pub fn kind_char(&self) -> char {
        match self.kind {
            KIND_VIDEO => 'V',
            KIND_LINK => 'L',
            _ => 'I',
        }
    }
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
                    let ok_host = MoveWindow(h.host, spec.x, spec.y, spec.w, spec.h, true).is_ok();
                    let ok_mag = MoveWindow(h.mag, 0, 0, spec.w, spec.h, true).is_ok();
                    let ok_src = MagSetWindowSource(h.mag, src).as_bool();
                    // alpha.6: these were silent — a failed source-set
                    // leaves the PREVIOUS region's content displayed at
                    // the new position (the "random rectangle" report).
                    if !ok_host || !ok_mag || !ok_src {
                        tracing::warn!(
                            "holes: #{} move result host={} mag={} src={} @({},{}) {}x{}",
                            i, ok_host, ok_mag, ok_src,
                            spec.x, spec.y, spec.w, spec.h
                        );
                    }
                }
            }

            unsafe {
                // Visibility reconcile — the pump is the single writer.
                // SHOW must go through SetWindowPos(HWND_TOPMOST, …):
                // a plain ShowWindow leaves the host at whatever Z
                // position it last had, and the freshly-activated
                // browser sits ABOVE it → invisible hole. Topmost band
                // cannot be covered by any normal app window.
                if want_visible && !h.visible {
                    let _ = SetWindowPos(
                        h.host,
                        HWND_TOPMOST,
                        0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    h.visible = true;
                    tracing::info!(
                        "holes: #{} shown @({},{}) {}x{} {}{}",
                        i, spec.x, spec.y, spec.w, spec.h,
                        spec.kind_char(),
                        if h.dynamic { " dyn" } else { "" }
                    );
                } else if !want_visible && h.visible {
                    let _ = ShowWindow(h.host, SW_HIDE_CMD);
                    h.visible = false;
                    tracing::info!("holes: #{} hidden", i);
                }

                // Tiered refresh: dynamic every tick, static ~1.7Hz.
                // The static cadence also re-asserts topmost — if any
                // other topmost popup slipped above us, the hole
                // recovers within ~600ms without user action.
                if want_visible
                    && h.visible
                    && (h.dynamic || tick.is_multiple_of(STATIC_EVERY))
                {
                    let ok = MagSetWindowSource(h.mag, h.source).as_bool();
                    if ok != h.src_ok {
                        // Edge-logged: one line per failure onset / recovery.
                        if ok {
                            tracing::info!("holes: #{} source recovered", i);
                        } else {
                            tracing::warn!(
                                "holes: #{} MagSetWindowSource FAILED @({},{}) {}x{} — content frozen",
                                i, h.source.left, h.source.top,
                                h.source.right - h.source.left,
                                h.source.bottom - h.source.top
                            );
                        }
                        h.src_ok = ok;
                    }
                    if tick.is_multiple_of(STATIC_EVERY) {
                        let _ = SetWindowPos(
                            h.host,
                            HWND_TOPMOST,
                            0, 0, 0, 0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        );
                    }
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
    /// Last MagSetWindowSource result — failures are logged on EDGE
    /// (ok→fail / fail→ok), never per tick (pump runs at 30ms; a failing
    /// call would otherwise spam 33 lines/second into log.txt).
    src_ok: bool,
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
        let host = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
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
            src_ok: true,
        });
    }

    HolePool { pool, tick: 0 }
}
