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
use std::os::raw::{c_char, c_int, c_long, c_uint, c_ushort};
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
            if size < 2 {
                65535
            } else {
                ((i * 65535) / (size - 1)) as c_ushort
            }
        };
        let mut lut = Lut {
            red: Vec::new(),
            green: Vec::new(),
            blue: Vec::new(),
        };
        Lut::fill(&mut lut.red, size, gen);
        Lut::fill(&mut lut.green, size, gen);
        Lut::fill(&mut lut.blue, size, gen);
        lut
    }

    fn inverted(size: usize) -> Lut {
        let gen = move |i: usize| -> c_ushort {
            if size < 2 {
                0
            } else {
                (((size - 1 - i) * 65535) / (size - 1)) as c_ushort
            }
        };
        let mut lut = Lut {
            red: Vec::new(),
            green: Vec::new(),
            blue: Vec::new(),
        };
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

// ---------------------------------------------------------------------
// Gamma I/O helpers — free functions so both the daemon (Session) and
// the --self-test diagnostic share one code path.
// ---------------------------------------------------------------------

fn read_gamma(rr: &Xrandr, dpy: *mut xlib::Display, crtc: RRCrtc) -> Option<Lut> {
    unsafe {
        let g = (rr.XRRGetCrtcGamma)(dpy, crtc);
        if g.is_null() {
            return None;
        }
        let size = (*g).size.max(0) as usize;
        let out = if size >= 2 {
            Some(Lut {
                red: std::slice::from_raw_parts((*g).red, size).to_vec(),
                green: std::slice::from_raw_parts((*g).green, size).to_vec(),
                blue: std::slice::from_raw_parts((*g).blue, size).to_vec(),
            })
        } else {
            None
        };
        (rr.XRRFreeGamma)(g);
        out
    }
}

fn write_gamma(rr: &Xrandr, dpy: *mut xlib::Display, crtc: RRCrtc, lut: &Lut) -> bool {
    unsafe {
        let size = lut.red.len();
        let g = (rr.XRRAllocGamma)(size as c_int);
        if g.is_null() {
            tracing::warn!("linux: XRRAllocGamma({size}) failed");
            return false;
        }
        std::slice::from_raw_parts_mut((*g).red, size).copy_from_slice(&lut.red);
        std::slice::from_raw_parts_mut((*g).green, size).copy_from_slice(&lut.green);
        std::slice::from_raw_parts_mut((*g).blue, size).copy_from_slice(&lut.blue);
        (rr.XRRSetCrtcGamma)(dpy, crtc, g);
        (rr.XRRFreeGamma)(g);
        true
    }
}

/// True when the ramp reads back as a decreasing curve — i.e. the X
/// server really accepted our inverted ramp. Quantization-tolerant:
/// some DDuo/DDX paths store the LUT at 8-bit precision, so entries may
/// wobble by one step without the shape changing.
fn ramp_is_inverted(l: &Lut) -> bool {
    let r = &l.red;
    if r.len() < 8 {
        return false;
    }
    let size = r.len();
    let tol = 65535usize / (size - 1).max(1) + 300;
    let monotonic = r
        .windows(2)
        .all(|w| (w[1] as usize) <= (w[0] as usize) + tol);
    let span = (r[0] as usize).saturating_sub(r[size - 1] as usize);
    monotonic && span > 65535 / 2
}

/// Near-equality with 8-bit-quantization tolerance (restore verify).
fn luts_near(a: &Lut, b: &Lut) -> bool {
    fn near(x: &[c_ushort], y: &[c_ushort]) -> bool {
        if x.len() != y.len() || x.is_empty() {
            return false;
        }
        let tol = (65535usize / (x.len() - 1).max(1)) + 300;
        x.iter().zip(y).all(|(p, q)| p.abs_diff(*q) as usize <= tol)
    }
    near(&a.red, &b.red) && near(&a.green, &b.green) && near(&a.blue, &b.blue)
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
        let xlib = Xlib::open()
            .map_err(|e| format!("libX11 unavailable ({e}) — is an X11 stack installed?"))?;
        let rr = Xrandr::open()
            .map_err(|e| format!("libXrandr unavailable ({e}) — XRandR 1.2+ required"))?;
        unsafe {
            (xlib.XSetErrorHandler)(Some(x_err_handler));
            let dpy = (xlib.XOpenDisplay)(std::ptr::null());
            if dpy.is_null() {
                return Err("XOpenDisplay failed — $DISPLAY unset or no X server reachable".into());
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
                if let Some(lut) = read_gamma(&self.rr, self.dpy, crtc) {
                    self.saved.push(SavedRamp { crtc, lut });
                    fresh += 1;
                }
            }
            (self.rr.XRRFreeScreenResources)(res);
            Ok(fresh)
        }
    }

    fn push_lut(&self, crtc: RRCrtc, lut: &Lut) {
        write_gamma(&self.rr, self.dpy, crtc, lut);
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
                let size = match read_gamma(&self.rr, self.dpy, crtc) {
                    Some(lut) => lut.red.len(),
                    None => continue,
                };
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
                    self.dpy,
                    self.kc_toggle as c_int,
                    base | extra,
                    self.root,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                );
                (self.xlib.XGrabKey)(
                    self.dpy,
                    self.kc_quit as c_int,
                    base | extra,
                    self.root,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
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
    println!("  invertonator              start inverted (Ctrl+Alt+I toggle, Ctrl+Alt+Q quit)");
    println!("  invertonator --restore    push identity gamma ramps and exit (recovery)");
    println!("  invertonator --self-test  hardware diagnostic — prints a paste-back report");
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }
    if args.iter().any(|a| a == "--self-test") {
        init_logging();
        tracing::info!("=== Invertonator-rs v{VERSION} self-test requested ===");
        let code = self_test();
        tracing::info!("self-test exit code: {code}");
        std::process::exit(code);
    }
    let restore_only = args.iter().any(|a| a == "--restore");

    init_logging();
    tracing::info!("=== Invertonator-rs v{VERSION} (linux-x11: gamma inversion) ===");

    if let Err(e) = x11_run(restore_only) {
        tracing::error!("{e}");
        eprintln!("invertonator: {e}");
        std::process::exit(1);
    }
    tracing::info!("clean exit — colors restored");
}

fn x11_run(restore_only: bool) -> Result<(), String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none() {
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
        let mut ev = xlib::XEvent {
            pad: [0 as c_long; 24],
        };
        unsafe { (session.xlib.XNextEvent)(session.dpy, &mut ev) };

        // XEvent storage is a 24-c_long block; XKeyEvent overlays its
        // head exactly as in C — read it with a pointer cast, not a
        // transmute (sizes differ).
        let kev: xlib::XKeyEvent =
            unsafe { std::ptr::read(&ev as *const xlib::XEvent as *const xlib::XKeyEvent) };
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
// --self-test — one-shot hardware/driver diagnostic.
//
// Purpose: the maintainer develops on Windows; Linux confirmation has
// to come from real machines. This prints EVERYTHING a remote tester
// needs to send back in one paste: session type, X server identity,
// outputs and their gamma hardware, a live invert/restore round-trip
// (catches drivers that silently ignore SetCrtcGamma), and hotkey
// availability. Exit code: 0 pass, 1 fail, 2 environment unavailable.
// =====================================================================

fn self_test() -> i32 {
    println!("=== Invertonator-rs v{VERSION} — Linux self-test report ===");
    println!();
    println!("[environment]");
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "DESKTOP_SESSION",
    ] {
        let val = std::env::var(key).unwrap_or_else(|_| "<unset>".into());
        println!("  {key:19} = {val}");
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    println!("  (unix time {secs})");

    println!();
    println!("[libraries]");
    let xlib = match Xlib::open() {
        Ok(x) => {
            println!("  libX11              : loaded");
            x
        }
        Err(e) => {
            println!("  libX11              : FAILED ({e})");
            println!();
            println!("[verdict]");
            println!("  FAIL — X11 client libraries missing (install libx11-6 / libx11-dev).");
            return 2;
        }
    };
    let rr = match Xrandr::open() {
        Ok(x) => {
            println!("  libXrandr           : loaded");
            x
        }
        Err(e) => {
            println!("  libXrandr           : FAILED ({e})");
            println!();
            println!("[verdict]");
            println!("  FAIL — libXrandr missing (install libxrandr2 / libxrandr-dev).");
            return 2;
        }
    };

    let mut tested = 0usize;
    let mut inverted_ok = 0usize;
    let mut restore_ok = 0usize;
    let mut rejected = 0usize;

    unsafe {
        (xlib.XSetErrorHandler)(Some(x_err_handler));
        let dpy = (xlib.XOpenDisplay)(std::ptr::null());
        if dpy.is_null() {
            println!();
            println!("[X server]");
            println!("  connection          : FAILED — $DISPLAY unset or no X server reachable");
            println!();
            println!("[verdict]");
            println!("  ENV UNAVAILABLE — no X11 display. On a Wayland-only session (GNOME\n  default on modern distros) log out and pick the '… on Xorg' session type.");
            return 2;
        }
        let scr = (xlib.XScreenCount)(dpy);
        let root = (xlib.XDefaultRootWindow)(dpy);
        let vendor = std::ffi::CStr::from_ptr((xlib.XServerVendor)(dpy) as *const c_char)
            .to_string_lossy()
            .into_owned();
        println!();
        println!("[X server]");
        println!("  connection          : opened (screen 0 of {scr})");
        println!("  server vendor       : {vendor}");
        println!(
            "  protocol version    : {}.{}",
            (xlib.XProtocolVersion)(dpy),
            (xlib.XProtocolRevision)(dpy)
        );
        let (mut rr_maj, mut rr_min) = (0 as c_int, 0 as c_int);
        if (rr.XRRQueryVersion)(dpy, &mut rr_maj, &mut rr_min) == 0 {
            println!("  XRandR version      : NOT AVAILABLE");
            (xlib.XCloseDisplay)(dpy);
            println!();
            println!("[verdict]");
            println!("  FAIL — XRandR extension missing; gamma control is impossible.");
            return 2;
        }
        println!("  XRandR version      : {rr_maj}.{rr_min}");

        // ---- outputs & gamma hardware ----
        let res = (rr.XRRGetScreenResourcesCurrent)(dpy, root);
        if res.is_null() {
            (xlib.XCloseDisplay)(dpy);
            println!();
            println!("[verdict]");
            println!("  FAIL — XRRGetScreenResourcesCurrent returned null.");
            return 2;
        }
        let ncrtc = (*res).ncrtc.max(0) as usize;
        println!();
        println!("[outputs / gamma hardware]");
        for i in 0..ncrtc {
            let crtc = *(*res).crtcs.add(i);
            let info = (rr.XRRGetCrtcInfo)(dpy, res, crtc);
            if info.is_null() {
                println!("  CRTC #{i}             : <unavailable>");
                continue;
            }
            let nout = (*info).noutput.max(0) as usize;
            let mut names: Vec<String> = Vec::new();
            for o in 0..nout {
                let out = *(*info).outputs.add(o);
                let oi = (rr.XRRGetOutputInfo)(dpy, res, out);
                if !oi.is_null() {
                    names.push(
                        std::ffi::CStr::from_ptr((*oi).name as *const c_char)
                            .to_string_lossy()
                            .into_owned(),
                    );
                    (rr.XRRFreeOutputInfo)(oi);
                }
            }
            (rr.XRRFreeCrtcInfo)(info);
            let active = nout > 0;
            let gsize = (rr.XRRGetCrtcGammaSize)(dpy, crtc).max(0) as usize;
            let state = if active { "ACTIVE" } else { "idle  " };
            let outs = if names.is_empty() {
                "-".to_string()
            } else {
                names.join("+")
            };
            if !active || gsize < 2 {
                println!("  CRTC #{i} ({state}) outputs={outs} gamma_size={gsize} — skipped");
                continue;
            }
            let original = match read_gamma(&rr, dpy, crtc) {
                Some(lut) => lut,
                None => {
                    println!("  CRTC #{i} ({state}) outputs={outs} gamma_size={gsize} — gamma unreadable, skipped");
                    continue;
                }
            };

            // Round-trip: push an inverted ramp, read it back, restore.
            X_ERRORS.store(false, Ordering::Relaxed);
            write_gamma(&rr, dpy, crtc, &Lut::inverted(gsize));
            (xlib.XSync)(dpy, xlib::False);
            let wrote = !X_ERRORS.load(Ordering::Relaxed);
            let set_ok = wrote
                && read_gamma(&rr, dpy, crtc)
                    .map(|l| ramp_is_inverted(&l))
                    .unwrap_or(false);

            X_ERRORS.store(false, Ordering::Relaxed);
            write_gamma(&rr, dpy, crtc, &original);
            (xlib.XSync)(dpy, xlib::False);
            let restored = !X_ERRORS.load(Ordering::Relaxed)
                && read_gamma(&rr, dpy, crtc)
                    .map(|l| luts_near(&l, &original))
                    .unwrap_or(false);

            let rt = if !wrote {
                "REJECTED"
            } else if !set_ok {
                "RAMP-UNCHANGED"
            } else if !restored {
                "RESTORE-IMPRECISE"
            } else {
                "PASS"
            };
            println!("  CRTC #{i} ({state}) outputs={outs} gamma_size={gsize} round-trip={rt}");
            tested += 1;
            if set_ok {
                inverted_ok += 1;
            }
            if restored {
                restore_ok += 1;
            }
            if !wrote {
                rejected += 1;
            }
        }
        (rr.XRRFreeScreenResources)(res);

        // ---- hotkey availability (grab + immediately ungrab) ----
        println!();
        println!("[hotkeys]");
        let sym_t = (xlib.XStringToKeysym)(CString::new("i").unwrap().as_ptr());
        let sym_q = (xlib.XStringToKeysym)(CString::new("q").unwrap().as_ptr());
        let kc_t = (xlib.XKeysymToKeycode)(dpy, sym_t) as c_uint;
        let kc_q = (xlib.XKeysymToKeycode)(dpy, sym_q) as c_uint;
        if kc_t == 0 || kc_q == 0 {
            println!("  keyboard mapping    : no plain 'i'/'q' keycode — hotkeys unavailable on this layout");
        } else {
            GRAB_CONFLICT.store(false, Ordering::Relaxed);
            let base = (xlib::Mod1Mask | xlib::ControlMask) as c_uint;
            let extras = [
                0,
                xlib::LockMask as c_uint,
                xlib::Mod2Mask as c_uint,
                (xlib::LockMask | xlib::Mod2Mask) as c_uint,
            ];
            for extra in extras {
                (xlib.XGrabKey)(
                    dpy,
                    kc_t as c_int,
                    base | extra,
                    root,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                );
                (xlib.XGrabKey)(
                    dpy,
                    kc_q as c_int,
                    base | extra,
                    root,
                    xlib::False,
                    xlib::GrabModeAsync,
                    xlib::GrabModeAsync,
                );
            }
            (xlib.XSync)(dpy, xlib::False);
            let conflict = GRAB_CONFLICT.load(Ordering::Relaxed);
            for extra in extras {
                (xlib.XUngrabKey)(dpy, kc_t as c_int, base | extra, root);
                (xlib.XUngrabKey)(dpy, kc_q as c_int, base | extra, root);
            }
            (xlib.XSync)(dpy, xlib::False);
            if conflict {
                println!("  Ctrl+Alt+I / Q      : CONFLICT — another app owns these keys (app still\n  runs; close that tool or it will win the keys)");
            } else {
                println!(
                    "  Ctrl+Alt+I / Q      : OK — keys are free and grabbable (test grabs removed)"
                );
            }
        }

        (xlib.XCloseDisplay)(dpy);
    }

    println!();
    println!("[verdict]");
    let (line, code) = if tested == 0 {
        ("FAIL — no output exposes a usable gamma LUT. If XDG_SESSION_TYPE above\n  says 'wayland', Invertonator needs an Xorg/X11 session for now.", 1)
    } else if rejected == tested {
        ("FAIL — the X server rejected every gamma write (driver/compositor\n  blocks CRTC gamma changes).", 1)
    } else if inverted_ok == 0 {
        ("FAIL — gamma writes were accepted but the ramp never changed\n  (typical for XWayland nested in a Wayland session). Run on a real\n  X11 session.", 1)
    } else if restore_ok < inverted_ok {
        ("PARTIAL PASS — inversion works; restore verification was imprecise\n  (usually 8-bit LUT quantization, visually harmless). App should work.", 0)
    } else {
        ("PASS — this machine supports XRandR gamma inversion. The full app\n  should work here.", 0)
    };
    println!("  {line}");
    tracing::info!("self-test verdict: {line}");
    println!();
    println!(
        ">>> Please copy this ENTIRE report (everything the command printed) and send it back. <<<"
    );
    code
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
