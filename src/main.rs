// Invertonator-rs — v0.3 cross-platform entry point.
//
//   windows: Magnification-API inversion + UIA media tracker (win_main.rs,
//            holes.rs, tracker.rs, scrollhook.rs)
//   linux:   X11 XRandR gamma-ramp inversion, global (linux.rs) — true-color
//            holes arrive with the XComposite overlay backend (v2)
//
// Shared code (settings, logging, version) lives in common.rs. Every
// platform module is cfg-gated so `cargo check` works on any host.

mod common;

#[cfg(windows)]
mod holes;
#[cfg(windows)]
mod scrollhook;
#[cfg(windows)]
mod tracker;
#[cfg(windows)]
mod win_main;

// X11 backend (works on any unix with Xlib — the common desktop case).
#[cfg(all(unix, not(target_os = "macos")))]
mod linux;

fn main() {
    #[cfg(windows)]
    win_main::run();

    #[cfg(all(unix, not(target_os = "macos")))]
    linux::run();

    #[cfg(target_os = "macos")]
    {
        // Gamma inversion is easy on macOS (CGSetDisplayTransferByTable)
        // but true-color holes need private APIs — unsupported for now.
        eprintln!("Invertonator-rs: macOS is not supported yet (see README platform matrix).");
        std::process::exit(2);
    }
}
