// Invertonator-rs — scrollhook: WH_MOUSE_LL wheel watch
//
//   The low-level mouse hook watches for wheel activity and raises a
//   short "hold burst". While the burst is live the hole pump keeps
//   every hole hidden — during scroll flicks the content under a hole
//   changes faster than the tracker can follow, and a hole pinned to
//   stale screen coords would invert the WRONG content.
//
//   LL hooks are delivered via the hook thread's message queue, so the
//   hook lives on a dedicated thread with its own message pump. The
//   hook proc itself only does an atomic store — never blocks the
//   input pipeline (a slow LL hook adds latency to EVERY keystroke
//   system-wide).
//
//   This covers wheel scrolling. Scrollbar dragging and keyboard
//   (PgDn/Space) scrolling are NOT hooked — the tracker's wake-on-
//   demand loop (alpha.10) plus fg-rect polling is the general answer
//   there.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, MSG, WH_MOUSE_LL,
    WM_MOUSEHWHEEL, WM_MOUSEWHEEL,
};

/// Holes stay hidden this long after the LAST wheel tick. Refreshed on
/// every wheel event, so continuous scrolling keeps holes parked and
/// they reappear ~this long after the flick ends.
pub const HOLD_BURST_MS: u64 = 350;

static HOLD_UNTIL_MS: AtomicU64 = AtomicU64::new(0);

/// alpha.10 — set on EVERY wheel event: the page content under the
/// holes is about to move, so the hole set is going stale. The tracker
/// consumes this flag and re-walks the moment the scroll burst ends
/// (walking DURING the burst is wasted browser tax — the pump keeps
/// the holes hidden anyway).
static PAGE_DIRTY: AtomicBool = AtomicBool::new(false);

/// True (once) if a wheel event fired since the last call.
pub fn take_page_dirty() -> bool {
    PAGE_DIRTY.swap(false, Ordering::Relaxed)
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// True while a scroll burst is live (pump consults this every tick).
pub fn hold_active() -> bool {
    now_ms() < HOLD_UNTIL_MS.load(Ordering::Relaxed)
}

unsafe extern "system" fn hook_proc(ncode: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ncode >= 0 {
        let msg = wparam.0 as u32;
        if msg == WM_MOUSEWHEEL || msg == WM_MOUSEHWHEEL {
            HOLD_UNTIL_MS.store(now_ms().saturating_add(HOLD_BURST_MS), Ordering::Relaxed);
            PAGE_DIRTY.store(true, Ordering::Relaxed);
        }
    }
    CallNextHookEx(None, ncode, wparam, lparam)
}

/// Installs the WH_MOUSE_LL hook on a dedicated thread (LL hook
/// callbacks need the installing thread to pump messages).
pub fn spawn() -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("scroll-hook".into())
        .spawn(move || unsafe {
            let hinstance = HINSTANCE::from(GetModuleHandleW(None).unwrap());
            let hook: HHOOK = match SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hinstance, 0) {
                Ok(h) => h,
                Err(e) => {
                    tracing::error!(
                        "scroll-hook: install failed: {e} — holes will flicker on scroll"
                    );
                    return;
                }
            };
            tracing::info!("scroll-hook: installed (burst {HOLD_BURST_MS}ms)");

            // LL hook events arrive on this thread's message queue —
            // pump until quit.
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}

            let _ = UnhookWindowsHookEx(hook);
            tracing::info!("scroll-hook: removed");
        })
        .expect("scroll-hook: thread spawn")
}
