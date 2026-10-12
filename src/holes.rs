// Invertonator-rs — holes: the true-color hole pool + refresh pump
//
//   Each hole = a layered topmost host window hosting a Magnifier-class
//   child. The child re-captures its source region and applies the
//   INVERT color effect — media under a hole passes through the OS
//   inversion twice and renders in TRUE colors.
//
//   THREADING MODEL:
//   · Holes::new() creates the pool — MAX_HOLES host+magnifier window
//     pairs, created on the MAIN thread (which runs the message loop
//     that pumps their messages), parked offscreen and hidden.
//   · The pool handles are then MOVED to the pump thread, which owns
//     them exclusively (PoolHole is unsafe-impl Send: we only use
//     async-style window calls, never cross-thread SendMessage, and
//     the pump is the sole owner — the standard window-ownership move).
//   · Single-writer visibility: only the pump changes hole visibility.
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
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const LWA_ALPHA_FLAG: LAYERED_WINDOW_ATTRIBUTES_FLAGS =
    LAYERED_WINDOW_ATTRIBUTES_FLAGS(2);
const SW_HIDE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(0);
const SW_SHOWNOACTIVATE_CMD: SHOW_WINDOW_CMD = SHOW_WINDOW_CMD(4);
const PUMP_MS: u64 = 30;          // pump cadence
const STATIC_EVERY: u64 = 20;     // static holes refresh ~1.7Hz
const MAX_HOLES: usize = 16;

// =====================================================================
// Public API
// =====================================================================

/// One desired hole. Coordinates are SCREEN physical pixels.
#[derive(Clone, Copy, Default, Debug)]
pub struct HoleSpec {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// true = refresh every pump tick (video-adjacent). false = ~1.7Hz.
    pub dynamic: bool,
}

/// Handle to the hole system.
///
/// Clone shares the pool state but NOT the pump thread — only the
/// original instance stops the pump on drop.
pub struct Holes {
    shared: Arc<HolesShared>,
    owns_pump: bool,
    pump_thread: Option<std::thread::JoinHandle<()>>,
}

impl Clone for Holes {
    fn clone(&self) -> Self {
        // Shares state; the clone does not own the pump thread.
        Self {
            shared: self.shared.clone(),
            owns_pump: false,
            pump_thread: None,
        }
    }
}

pub struct HolesShared {
    // The desired hole set — written by the tracker, read by the pump.
    pub specs: Mutex<Vec<HoleSpec>>,
    pub active: AtomicBool,   // false = holes hidden (Dim/Off)
    pub pierce: AtomicBool,
    pub hold: AtomicBool,     // scroll-hold (holes hidden during flicks)
    pub run: AtomicBool,
}

impl Holes {
    /// Creates the pool (MAX_HOLES host+magnifier window pairs, parked
    /// offscreen and hidden) and starts the pump thread. MUST be called
    /// from the thread that will run the message loop (main) — the hole
    /// windows' messages are pumped by main.
    pub fn new() -> Self {
        unsafe {
            let hinstance = HINSTANCE::from(GetModuleHandleW(None).unwrap());

            // Host window class — click-through via WM_NCHITTEST.
            let wc = WNDCLASSW {
                lpfnWndProc: Some(hole_host_wndproc),
                hInstance: hinstance,
                lpszClassName: HOST_CLASS,
                ..Default::default()
            };
            // "already registered" is fine — idempotent per process.
            RegisterClassW(&wc);

            let mut pool: Vec<PoolHole> = Vec::with_capacity(MAX_HOLES);
            for _ in 0..MAX_HOLES {
                let host = CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW
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
                MagSetColorEffect(mag, INVERT_FLAT.as_ptr());
                // 1:1 transform — show the source region unscaled.
                MagSetWindowTransform(mag, IDENTITY_9.as_ptr());

                pool.push(PoolHole {
                    host,
                    mag,
                    source: RECT::default(),
                    visible: false,
                    dynamic: false,
                });
            }

            tracing::info!("holes: pool created ({} windows)", pool.len());

            let shared = Arc::new(HolesShared {
                specs: Mutex::new(Vec::new()),
                active: AtomicBool::new(true),
                pierce: AtomicBool::new(false),
                hold: AtomicBool::new(false),
                run: AtomicBool::new(true),
            });

            let pump_shared = shared.clone();
            let pump_pool = pool;
            let pump_thread = std::thread::Builder::new()
                .name("holes-pump".into())
                .spawn(move || pump_loop(pump_shared, pump_pool))
                .expect("holes: pump thread spawn");

            Self {
                shared,
                owns_pump: true,
                pump_thread: Some(pump_thread),
            }
        }
    }

    /// Replace the desired hole set (screen physical px). Call at
    /// discovery cadence. Surplus holes are hidden automatically.
    pub fn set_specs(&self, specs: Vec<HoleSpec>) {
        *self.shared.specs.lock().unwrap() = specs;
    }

    /// Scroll-hold: holes vanish instantly while set (wheel activity).
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

impl Drop for Holes {
    fn drop(&mut self) {
        // Only the original instance (which owns the pump thread) stops it.
        if self.owns_pump {
            self.shared.run.store(false, Ordering::Relaxed);
            if let Some(t) = self.pump_thread.take() {
                let _ = t.join();
            }
        }
    }
}

// =====================================================================
// Pump internals — the pump thread OWNS the hole windows.
// =====================================================================

struct PoolHole {
    host: HWND,
    mag: HWND,
    // pump-local last-applied state (single writer: the pump)
    source: RECT,
    visible: bool,
    dynamic: bool,
}

// SAFETY: PoolHole holds only window HANDLES. We move ownership to the
// pump thread (never share), and the pump uses only async-style window
// calls (MoveWindow / ShowWindow / MagSet*Source) — never cross-thread
// SendMessage. This is the standard window-ownership transfer pattern.
unsafe impl Send for PoolHole {}

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

fn pump_loop(shared: Arc<HolesShared>, mut pool: Vec<PoolHole>) {
    let mut tick: u64 = 0;

    while shared.run.load(Ordering::Relaxed) {
        tick += 1;

        // want_visible — the pump is the SINGLE writer of hole visibility.
        let want_visible = shared.active.load(Ordering::Relaxed)
            && !shared.pierce.load(Ordering::Relaxed)
            && !shared.hold.load(Ordering::Relaxed);

        let specs = shared.specs.lock().unwrap().clone();

        for (i, spec) in specs.iter().enumerate() {
            if i >= pool.len() {
                break;
            }
            let h = &mut pool[i];

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
                } else if !want_visible && h.visible {
                    let _ = ShowWindow(h.host, SW_HIDE_CMD);
                    h.visible = false;
                }

                // Tiered refresh: dynamic every tick, static ~1.7Hz.
                if want_visible && h.visible && (h.dynamic || tick % STATIC_EVERY == 0) {
                    let _ = MagSetWindowSource(h.mag, h.source);
                }
            }
        }

        // Surplus holes: hide.
        for h in pool.iter_mut().skip(specs.len()) {
            if h.visible {
                unsafe {
                    let _ = ShowWindow(h.host, SW_HIDE_CMD);
                }
                h.visible = false;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(PUMP_MS));
    }
}