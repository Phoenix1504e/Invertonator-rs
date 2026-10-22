// Invertonator-rs — Linux X11 backend (v0.3: global gamma inversion).
//
//   Windows inverts inside the DWM via the Magnifier API. X11 has no
//   equivalent userland hook, but it exposes the hardware gamma LUT
//   (XRandR 1.2 CRTC gamma): remapping [0..255] → [255..0] inverts
//   every pixel at scanout — *after* the browser rendered, so the web
//   layer still sees nothing (same anti-detection property).
//
//   v1 scope — honest limitations:
//     * GLOBAL inversion: media is inverted too. True-color holes need
//       the v2 XComposite + XShm overlay backend (browser window
//       pixmaps stay un-inverted, so hole regions can be blitted in
//       original colors on an ARGB overlay — design note at EOF).
//     * X11 sessions only. Native Wayland needs compositor backends
//       (KWin effect / GNOME extension) — on the roadmap.
//     * If the process is SIGKILLed while inverted the ramp persists;
//       run `invertonator --restore` to reset to identity.
//
// x11-dl dlopens libX11 at runtime, so the binary builds with zero
// native build-dependencies and runs on any desktop with X present.

use std::ffi::CString;
use std::os::raw::{c_int, c_long, c_uint, c_ushort};
use std::sync::atomic::{AtomicBool, Ordering};

use x11_dl::xlib::{self, Xlib};
use x11_dl::xrandr::{RRCrtc, Xrandr};

use crate::common::{init_logging, VERSION};

// =====================================================================
// X error capture — the DEFAULT Xlib error handler ABORTS the process.
// Every protocol error (e.g. BadAccess from a conflicting XGrabKey or
// BadMatch from an unsupported gamma set) must come to us instead.
// =====================================================================

static X_ERRORS: AtomicBool = AtomicBool::new(false);
static GRAB_CONFLICT: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn x_err_handler(_dpy: *mut xlib::Display, ev: *mut xlib::XErrorEvent) -> c_int {
    if !ev.is_null() {
        let code = (*ev).error_code;
        if code == 10 {
            // BadAccess — almost always the hotkey grabs colliding.
            GRAB_CONFLICT.store(true, Ordering::Relaxed);
        } else {
            X_ERRORS.store(true, Ordering::Relaxed);
        }
        tracing::debug!("x11: protocol error {code}");
    }
    0
}

// =====================================================================
// Gamma ramps
// =====================================================================

#[derive(Clone)]
struct Lut {
    red: Vec<c_ushort>,
    green: Vec<c_ushort>,
    blue: Vec<c_ushort>,
}

impl Lut {
    fn fill(v: &mut Vec<c_ushort>, size: usize, f: impl Fn(usize) -> c_ushort) {
        v.clear();
        v.extend((0..size).map(f));
    }

    fn identity(size: usize) -> Lut {
        // size < 2 (degenerate LUT): full-on is the least surprising.
        let gen = move |i: usize| -> c_ushort {
            if size < 2 { 65535 } else { ((i * 65535) / (size - 1)) as c_ushort }
        };
        let mut lut = Lut { red: Vec::new(), green: Vec::new(), blue: Vec::new() };
        Lut::fill(&mut lut.red, size, gen);
        Lut::fill(&mut lut.green, size, gen);
        Lut::fill(&mut lut.blue, size, gen);
        lut
    }

    fn inverted(size: usize) -> Lut {
        let gen = move |i: usize| -> c_ushort {
            if size < 2 { 0 } else { (((size - 1 - i) * 65535) / (size - 1)) as c_ushort }
        };
        let mut lut = Lut { red: Vec::new(), green: Vec::new(), blue: Vec::new() };
        Lut::fill(&mut lut.red, size, gen);
        Lut::fill(&mut lut.green, size, gen);
        Lut::fill(&mut lut.blue, size, gen);
        lut
    }
}

/// The ORIGINAL ramp of one CRTC, captured before we first invert it.
struct SavedRamp {
    crtc: RRCrtc,
    lut: Lut,
}

// =====================================================================
// Session — owns the display connection; Drop restores all ramps.
// =====================================================================

struct Session {
    xlib: Xlib,
    rr: Xrandr,
    dpy: *mut xlib::Display,
    root: xlib::Window,
    saved: Vec<SavedRamp>,
    inverted: bool,
    kc_toggle: c_uint,
    kc_quit: c_uint,
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            self.push_all(Lut::identity, "restore-on-exit");
            if !self.dpy.is_null() {
                let _ = (self.xlib.XCloseDisplay)(self.dpy);
            }
        }
        tracing::info!("linux: display closed — original gamma ramps restored");
    }
}

impl Session {
    fn open() -> Result<Session, String> {
        let xlib = Xlib::open().map_err(|e| {
            format!("libX11 unavailable ({e}) — is an X11 stack installed?")
        })?;
        let rr = Xrandr::open()
            .map_err(|e| format!("libXrandr unavailable ({e}) — XRandR 1.2+ required"))?;
        unsafe {
            (xlib.XSetErrorHandler)(Some(x_err_handler));
            let dpy = (xlib.XOpenDisplay)(std::ptr::null());
            if dpy.is_null() {
                return Err(
                    "XOpenDisplay failed — $DISPLAY unset or no X server reachable".into()
                );
            }
            let root = (xlib.XDefaultRootWindow)(dpy);
            Ok(Session {
                xlib,
                rr,
                dpy,
                root,
                saved: Vec::new(),
                inverted: false,
                kc_toggle: 0,
                kc_quit: 0,
            })
        }
    }

    /// Capture current ramps as the originals. CRTCs already in `saved`
    /// are skipped so an inverted ramp is never mistaken for an
    /// original. Call only while NOT inverted.
    fn save_originals(&mut self) -> Result<usize, String> {
        debug_assert!(!self.inverted, "save_originals while inverted");
        unsafe {
            let res = (self.rr.XRRGetScreenResourcesCurrent)(self.dpy, self.root);
            if res.is_null() {
                return Err("XRRGetScreenResourcesCurrent failed".into());
            }
            let n = (*res).ncrtc.max(0) as usize;
            let mut fresh = 0usize;
            for i in 0..n {
                let crtc = *(*res).crtcs.add(i);
                if self.saved.iter().any(|s| s.crtc == crtc) {
                    continue;
                }
                let g = (self.rr.XRRGetCrtcGamma)(self.dpy, crtc);
                if g.is_null() {
                    continue;
                }
                let size = (*g).size.max(0) as usize;
                if size >= 2 {
                    let red = std::slice::from_raw_parts((*g).red, size).to_vec();
                    let green = std::slice::from_raw_parts((*g).green, size).to_vec();
                    let blue = std::slice::from_raw_parts((*g).blue, size).to_vec();
                    self.saved.push(SavedRamp { crtc, lut: Lut { red, green, blue } });
                    fresh += 1;
                }
                (self.rr.XRRFreeGamma)(g);
            }
            (self.rr.XRRFreeScreenResources)(res);
            Ok(fresh)
        }
    }

    fn push_lut(&self, crtc: RRCrtc, lut: &Lut) {
        unsafe {
            let size = lut.red.len();
            let g = (self.rr.XRRAllocGamma)(size as c_int);
            if g.is_null() {
                tracing::warn!("linux: XRRAllocGamma({size}) failed");
                return;
            }
            std::slice::from_raw_parts_mut((*g).red, size).copy_from_slice(&lut.red);
            std::slice::from_raw_parts_mut((*g).green, size).copy_from_slice(&lut.green);
            std::slice::from_raw_parts_mut((*g).blue, size).copy_from_slice(&lut.blue);
            (self.rr.XRRSetCrtcGamma)(self.dpy, crtc, g);
            (self.rr.XRRFreeGamma)(g);
        }
    }

    fn push_all(&self, make: impl Fn(usize) -> Lut, what: &str) {
        X_ERRORS.store(false, Ordering::Relaxed);
        let count = self.saved.len();
        for s in &self.saved {
            self.push_lut(s.crtc, &make(s.lut.red.len()));
        }
        unsafe { (self.xlib.XSync)(self.dpy, xlib::False) };
        if X_ERRORS.load(Ordering::Relaxed) {
            tracing::warn!(
                "linux: X11 error during {what} — driver may not support CRTC gamma \
                 (needs XRandR 1.2+; check with `xrandr --verbose`)"
            );
        }
        tracing::info!("linux: {what} pushed to {count} CRTC(s)");
    }

    fn toggle(&mut self) {
        if self.inverted {
            self.push_all(Lut::identity, "invert OFF");
            self.inverted = false;
        } else {
            // Pick up hot-plugged CRTCs while it is safe (not inverted).
            match self.save_originals() {
                Ok(fresh) if fresh > 0 => {
                    tracing::info!("linux: {fresh} new CRTC(s) now managed");
                }
                Err(e) => tracing::warn!("linux: re-enumeration failed: {e}"),
                _ => {}
            }
            self.push_all(Lut::inverted, "invert ON");
            self.inverted = true;
        }
        let msg = if self.inverted {
            "inversion ON — screen inverted (Ctrl+Alt+I to undo)"
        } else {
            "inversion OFF — colors restored"
        };
        tracing::info!("linux: {msg}");
        println!("Invertonator: {msg}");
    }

    /// --restore: push identity to every CRTC on the server, regardless
    /// of anything this or a crashed earlier instance did.
    fn reset_identity(&self) -> Result<usize, String> {
        unsafe {
            let res = (self.rr.XRRGetScreenResourcesCurrent)(self.dpy, self.root);
            if res.is_null() {
                return Err("XRRGetScreenResourcesCurrent failed".into());
            }
            let n = (*res).ncrtc.max(0) as usize;
            let mut count = 0usize;
            for i in 0..n {
                let crtc = *(*res).crtcs.add(i);
                let g = (self.rr.XRRGetCrtcGamma)(self.dpy, crtc);
                if g.is_null() {
                    continue;
                }
                let size = (*g).size.max(0) as usize;
                (self.rr.XRRFreeGamma)(g);
                if size < 2 {
                    continue;
                }
                self.push_lut(crtc, &Lut::identity(size));
                count += 1;
            }
            (self.rr.XRRFreeScreenResources)(res);
            Ok(count)
        }
    }

    fn grab_hotkeys(&mut self) -> Result<(), String> {
        unsafe {
            let sym_t = (self.xlib.XStringToKeysym)(CString::new("i").unwrap().as_ptr());
            let sym_q = (self.xlib.XStringToKeysym)(CString::new("q").unwrap().as_ptr());
            if sym_t == 0 || sym_q == 0 {
                return Err("XStringToKeysym failed for i/q".into());
            }
            self.kc_toggle = (self.xlib.XKeysymToKeycode)(self.dpy, sym_t) as c_uint;
            self.kc_quit = (self.xlib.XKeysymToKeycode)(self.dpy, sym_q) as c_uint;
            if self.kc_toggle == 0 || self.kc_quit == 0 {
                return Err("keyboard layout exposes no plain 'i'/'q' keycode".into());
            }
            // Grab every CapsLock/NumLock combination (the classic 4-combo
            // pattern) or the grab silently misses when a lock key is on.
            let base = (xlib::Mod1Mask | xlib::ControlMask) as c_uint;
            let extras = [
                0,
                xlib::LockMask as c_uint,
                xlib::Mod2Mask as c_uint,
                (xlib::LockMask | xlib::Mod2Mask) as c_uint,
            ];
            for extra in extras {
                (self.xlib.XGrabKey)(
                    self.dpy, self.kc_toggle as c_int, base | extra, self.root,
                    xlib::False, xlib::GrabModeAsync, xlib::GrabModeAsync,
                );
                (self.xlib.XGrabKey)(
                    self.dpy, self.kc_quit as c_int, base | extra, self.root,
                    xlib::False, xlib::GrabModeAsync, xlib::GrabModeAsync,
                );
            }
            (self.xlib.XSync)(self.dpy, xlib::False);
        }
        if GRAB_CONFLICT.load(Ordering::Relaxed) {
            tracing::warn!(
                "linux: hotkey grab conflicted — another app owns Ctrl+Alt+I/Q; \
                 inversion still runs, but the toggle may be unavailable"
            );
        }
        Ok(())
    }
}

// =====================================================================
// Entry
// =====================================================================

fn print_usage() {
    println!("Invertonator-rs v{VERSION} — linux-x11 backend");
    println!();
    println!("  invertonator            start inverted (Ctrl+Alt+I toggle, Ctrl+Alt+Q quit)");
    println!("  invertonator --restore  push identity gamma ramps and exit (recovery)");
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    let restore_only = args.iter().any(|a| a == "--restore");

    init_logging();
    tracing::info!(
        "=== Invertonator-rs v{VERSION} (linux-x11: gamma inversion) ==="
    );

    if let Err(e) = x11_run(restore_only) {
        tracing::error!("{e}");
        eprintln!("invertonator: {e}");
        std::process::exit(1);
    }
    tracing::info!("clean exit — colors restored");
}

fn x11_run(restore_only: bool) -> Result<(), String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_none()
    {
        return Err(
            "Wayland session detected (WAYLAND_DISPLAY set, DISPLAY empty). \
             This backend needs an X11 session — native Wayland support \
             (KWin effect / GNOME extension) is on the roadmap."
                .into(),
        );
    }

    let mut session = Session::open().map_err(|e| format!("x11 init: {e}"))?;

    if restore_only {
        let n = session.reset_identity()?;
        let msg = format!("--restore: identity ramps pushed to {n} CRTC(s)");
        tracing::info!("linux: {msg}");
        println!("Invertonator: {msg}");
        return Ok(());
    }

    // Start in the same state as the Windows build: inverted ON.
    session
        .save_originals()
        .map_err(|e| format!("gamma enumeration: {e}"))?;
    session.push_all(Lut::inverted, "invert ON");
    session.inverted = true;
    session.grab_hotkeys()?;

    let msg = "ready — Ctrl+Alt+I toggles inversion, Ctrl+Alt+Q quits";
    tracing::info!("linux: {msg}");
    println!("Invertonator v{VERSION} (linux-x11): {msg}");

    loop {
        let mut ev = xlib::XEvent { pad: [0 as c_long; 24] };
        unsafe { (session.xlib.XNextEvent)(session.dpy, &mut ev) };

        // XEvent storage is a 24-c_long block; XKeyEvent overlays its
        // head exactly as in C — read it with a pointer cast, not a
        // transmute (sizes differ).
        let kev: xlib::XKeyEvent = unsafe {
            std::ptr::read(&ev as *const xlib::XEvent as *const xlib::XKeyEvent)
        };
        if kev.type_ != xlib::KeyPress {
            continue;
        }
        let want = (xlib::ControlMask | xlib::Mod1Mask) as c_uint;
        if kev.state & want != want {
            continue;
        }
        if kev.keycode == session.kc_toggle {
            session.toggle();
        } else if kev.keycode == session.kc_quit {
            tracing::info!("linux: quit requested");
            println!("Invertonator: quitting — colors restored");
            break;
        }
    }

    Ok(()) // Drop(Session) restores the saved ramps
}

// =====================================================================
// v2 DESIGN NOTE — true-color holes on X11 (XComposite + XShm):
//
// The gamma LUT is applied at hardware scanout, so framebuffer pixels
// read via XShmGetImage are the ORIGINAL colors. Plan:
//   1. gamma-invert the whole screen (this backend, already done);
//   2. XComposite-redirect every browser top-level window;
//   3. for each tracked media rect, blit that region from the
//      browser's NameWindowPixmap (un-inverted) onto an always-on-top
//      ARGB override-redirect overlay window positioned over the rect
//      (input shape emptied, _NET_WM_STATE_ABOVE);
//   4. overlay pixels = original colors → media appears true-color,
//      everything around it stays inverted;
//   5. media discovery via AT-SPI2 (the Linux accessibility bus both
//      Chromium and Gecko expose) replaces the Windows UIA tracker.
// Capturing the browser's redirected pixmap — not the root — avoids
// the overlay-capturing-itself feedback loop.
// =====================================================================
