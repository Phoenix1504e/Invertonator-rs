// Invertonator — Milestone 1: Rust NegativeScreen with settings
//
//   Fullscreen color transforms via the Windows Magnification API:
//     · Invert — output = 1 - input
//     · Dim    — output = dimLevel · input (blacks preserved)
//     · Off    — identity
//   Tray icon, global hotkeys (Ctrl+Alt+F5 cycle / Ctrl+Alt+F2 quit),
//   settings.json (auto-created, C#-edition-compatible format).
//
//   NO HOLES YET — Milestone 2 (UIA tracker) and 3 (hole pool).
//
//   LESSON (ledger): the windows crate wraps CreateWindowExW in Result
//   and checks GetLastError() — but the API can return a valid HWND with
//   last-error 0, producing "failed: The operation completed successfully".
//   We call the raw function and null-check ourselves instead.

// #![windows_subsystem = "windows"]   // ← uncomment for release (no console)

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM, HINSTANCE};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NOTIFYICONDATAW,
    NIM_ADD, NIM_DELETE, NIM_MODIFY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, DispatchMessageW, GetMessageW, LoadCursorW,
    LoadImageW, RegisterClassW, TranslateMessage, DestroyWindow, PostQuitMessage,
    HICON, IMAGE_ICON, LR_DEFAULTSIZE, MSG,
    WNDCLASSW, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, CW_USEDEFAULT, WM_DESTROY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWLP_USERDATA,
};

// =====================================================================
// Settings — same settings.json format as the C# edition.
// =====================================================================

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct Settings {
    pub hotkeys: HotkeySettings,
    pub features: FeatureSettings,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct HotkeySettings {
    pub invert: String,
    pub mode: String,
    pub pierce: String,
    pub quit: String,
    pub dump: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        Self {
            invert: "Ctrl+Alt+F5".into(),
            mode: "Ctrl+Alt+F6".into(),
            pierce: "Ctrl+Alt+F7".into(),
            quit: "Ctrl+Alt+F8".into(),
            dump: "Ctrl+Alt+F9".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
struct FeatureSettings {
    pub at_mode: bool,
    pub dim_level: f64,
}

impl Default for FeatureSettings {
    fn default() -> Self {
        Self { at_mode: false, dim_level: 0.4 }
    }
}

impl Settings {
    fn path() -> PathBuf {
        let exe = std::env::current_exe().unwrap_or_default();
        exe.parent()
            .map(|p| p.join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("settings.json"))
    }

    fn load() -> Self {
        let path = Self::path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            match serde_json::from_str(&text) {
                Ok(s) => {
                    tracing::info!("settings: loaded from settings.json");
                    return s;
                }
                Err(e) => tracing::warn!("settings: parse failed ({e}) — defaults"),
            }
        }
        let defaults = Self::default();
        defaults.save();
        defaults
    }

    fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), text);
            tracing::info!("settings: saved");
        }
    }
}

// =====================================================================
// Magnification engine
// =====================================================================

mod magnification {
    use windows::Win32::Foundation::BOOL;

    #[link(name = "Magnification")]
    extern "system" {
        pub fn MagInitialize() -> BOOL;
        pub fn MagUninitialize() -> BOOL;
        pub fn MagSetFullscreenColorEffect(matrix: *const f32) -> BOOL;
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum ScreenMode {
        Invert,
        Dim,
        Off,
    }

    /// output = 1 - input (per channel)
    pub const INVERT: [[f32; 5]; 5] = [
        [-1.0,  0.0,  0.0, 0.0, 0.0],
        [ 0.0, -1.0,  0.0, 0.0, 0.0],
        [ 0.0,  0.0, -1.0, 0.0, 0.0],
        [ 0.0,  0.0,  0.0, 1.0, 0.0],
        [ 1.0,  1.0,  1.0, 0.0, 1.0],
    ];

    pub const IDENTITY: [[f32; 5]; 5] = [
        [1.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 0.0, 1.0],
    ];

    /// output = d · input — blacks stay black, whites dim to d.
    pub fn dim_matrix(d: f32) -> [[f32; 5]; 5] {
        [
            [d,   0.0, 0.0, 0.0, 0.0],
            [0.0, d,   0.0, 0.0, 0.0],
            [0.0, 0.0, d,   0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn initialize() -> bool {
        unsafe { MagInitialize().as_bool() }
    }

    pub fn uninitialize() {
        unsafe {
            let _ = MagUninitialize();
        }
    }

    pub fn set_fullscreen_effect(matrix: &[[f32; 5]; 5]) -> bool {
        unsafe { MagSetFullscreenColorEffect(matrix.as_ptr().cast::<f32>()).as_bool() }
    }
}

// =====================================================================
// RAW CreateWindowExW — the windows crate's Result-wrapped version has a
// quirk: a valid HWND return can carry last-error 0 ("success") and get
// converted to Err. Raw call + null-check is the correct pattern.
// =====================================================================

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

// =====================================================================
// Application state
// =====================================================================

use magnification::ScreenMode;

struct AppState {
    mode: ScreenMode,
    invert_on: AtomicBool,
    dim_level_milli: AtomicUsize,   // dim level × 1000 (atomic-friendly)
    hwnd: HWND,
    tray_nid: NOTIFYICONDATAW,
}

impl AppState {
    fn apply_mode(&self) {
        let invert_on = self.invert_on.load(Ordering::Relaxed);
        let mode = self.mode;
        let matrix = if !invert_on || mode == ScreenMode::Off {
            magnification::IDENTITY
        } else if mode == ScreenMode::Dim {
            let d = (self.dim_level_milli.load(Ordering::Relaxed) as f32) / 1000.0;
            magnification::dim_matrix(d)
        } else {
            magnification::INVERT
        };
        let ok = magnification::set_fullscreen_effect(&matrix);
        tracing::info!("apply mode={:?} invert_on={} ok={}", mode, invert_on, ok);
    }

    fn cycle_mode(&mut self) {
        let next = match self.mode {
            ScreenMode::Invert => ScreenMode::Dim,
            ScreenMode::Dim => ScreenMode::Off,
            ScreenMode::Off => ScreenMode::Invert,
        };
        self.mode = next;
        self.apply_mode();
    }

    fn toggle(&mut self) {
        let now = !self.invert_on.load(Ordering::Relaxed);
        self.invert_on.store(now, Ordering::Relaxed);
        self.apply_mode();
    }

    fn mode_text(&self) -> String {
        if !self.invert_on.load(Ordering::Relaxed) {
            return "Off".into();
        }
        match self.mode {
            ScreenMode::Invert => "Invert".into(),
            ScreenMode::Dim => "Dim".into(),
            ScreenMode::Off => "Off".into(),
        }
    }
}

// =====================================================================
// Tray + hotkeys
// =====================================================================

const WM_TRAYICON: u32 = 0x8000; // WM_APP
const WM_LBUTTONUP_TRAY: usize = 0x0202;

static TOGGLE_REQUEST: AtomicBool = AtomicBool::new(false);
static QUIT_REQUEST: AtomicBool = AtomicBool::new(false);
static CYCLE_REQUEST: AtomicBool = AtomicBool::new(false);

const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_QUIT: i32 = 2;
const HOTKEY_CYCLE: i32 = 3;
const HOTKEY_MODS: HOT_KEY_MODIFIERS = HOT_KEY_MODIFIERS(0x1 | 0x2 | 0x4000); // Ctrl+Alt+NoRepeat
const VK_F5: u32 = 0x74;
const VK_F2: u32 = 0x71;
const VK_F6: u32 = 0x75;

unsafe fn register_hotkeys(hwnd: HWND) -> bool {
    let register = |id: i32, vk: u32, name: &str| -> bool {
        let ok = RegisterHotKey(Some(&hwnd), id, HOTKEY_MODS, vk).is_ok();
        if ok {
            tracing::info!("hotkey: {name} registered");
        } else {
            tracing::error!("hotkey: {name} registration FAILED");
        }
        ok
    };
    let a = register(HOTKEY_TOGGLE, VK_F5, "invert");
    let b = register(HOTKEY_CYCLE, VK_F6, "mode");
    let c = register(HOTKEY_QUIT, VK_F2, "quit");
    a && b && c
}

/// SAFE tray-tip write: szTip is a fixed [u16; 128] — write the string,
/// then null-terminate. (The old `copy_from_slice(&wide[..128])` panicked
/// whenever the tip was shorter than 128 units — i.e., always.)
unsafe fn update_tray_tip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().take(127).collect();
    nid.szTip[..wide.len()].copy_from_slice(&wide);
    nid.szTip[wide.len()] = 0;
    let _ = Shell_NotifyIconW(NIM_MODIFY, nid);
}

// =====================================================================
// Window procedure
// =====================================================================

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;

    if !state_ptr.is_null() {
        let state = &mut *state_ptr;

        if msg == WM_TRAYICON && (lparam.0 & 0xFFFF) as usize == WM_LBUTTONUP_TRAY {
            TOGGLE_REQUEST.store(true, Ordering::Relaxed);
        }

        match msg {
            0x0312 /* WM_HOTKEY */ => {
                match wparam.0 as i32 {
                    HOTKEY_TOGGLE => TOGGLE_REQUEST.store(true, Ordering::Relaxed),
                    HOTKEY_CYCLE => CYCLE_REQUEST.store(true, Ordering::Relaxed),
                    HOTKEY_QUIT => QUIT_REQUEST.store(true, Ordering::Relaxed),
                    _ => {}
                }
            }
            WM_DESTROY => {
                tracing::info!("window destroyed — quitting");
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => {}
        }

        if TOGGLE_REQUEST.swap(false, Ordering::Relaxed) {
            state.toggle();
            let tip_text = format!("Invertonator — {}", state.mode_text());
            update_tray_tip(&mut state.tray_nid, &tip_text);
        }
        if CYCLE_REQUEST.swap(false, Ordering::Relaxed) {
            state.cycle_mode();
            let tip_text = format!("Invertonator — {}", state.mode_text());
            update_tray_tip(&mut state.tray_nid, &tip_text);
        }
        if QUIT_REQUEST.swap(false, Ordering::Relaxed) {
            tracing::info!("quit requested");
            DestroyWindow(hwnd).ok();
        }
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

// =====================================================================
// main
// =====================================================================

fn main() {
    // Logging: %LOCALAPPDATA%\Invertonator\log.txt
    let log_dir = dirs::data_local_dir()
        .unwrap_or_default()
        .join("Invertonator");
    std::fs::create_dir_all(&log_dir).ok();
    let log_file = tracing_appender::rolling::never(log_dir, "log.txt");
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_ansi(false)
        .init();

    tracing::info!("=== Invertonator-rs v0.1.0 (Milestone 1) ===");

    let settings = Settings::load();
    tracing::info!("settings: dimLevel={}", settings.features.dim_level);

    unsafe {
        if !magnification::initialize() {
            tracing::error!("MagInitialize failed — is Magnification.dll present?");
            return;
        }

        let hinstance = HINSTANCE::from(GetModuleHandleW(None).unwrap());
        let class_name = windows::core::w!("InvertonatorClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        RegisterClassW(&wc);

        // RAW CreateWindowExW + null-check (see ledger note at top).
        let hwnd = CreateWindowExW(
            0,                                        // dwExStyle
            class_name.as_ptr(),
            windows::core::w!("Invertonator").as_ptr(),
            0,                                        // style — hidden, tray-only
            CW_USEDEFAULT, CW_USEDEFAULT,
            CW_USEDEFAULT, CW_USEDEFAULT,
            HWND::default(),                          // no parent
            0,                                        // no menu
            hinstance,
            std::ptr::null_mut(),
        );
        if hwnd.is_invalid() {
            let err = std::io::Error::last_os_error();
            tracing::error!("CreateWindowExW failed: {err}");
            return;
        }

        // Icon: icon.ico next to the exe, if present
        let hicon: HICON = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("icon.ico")))
            .filter(|p| p.exists())
            .and_then(|p| {
                let wide: Vec<u16> = p.as_os_str().to_string_lossy()
                    .encode_utf16().chain([0]).collect();
                LoadImageW(
                    Some(&hinstance),
                    windows::core::PCWSTR(wide.as_ptr()),
                    IMAGE_ICON, 0, 0,
                    LR_DEFAULTSIZE,
                )
                .ok()
                .map(|h| HICON(h.0))
            })
            .unwrap_or_default();

        // AppState — handed to the window via GWLP_USERDATA. wndproc reads
        // it back with GetWindowLongPtrW (the C# pattern, raw-pointer form).
        let state_ptr = Box::into_raw(Box::new(AppState {
            mode: ScreenMode::Invert,
            invert_on: AtomicBool::new(true),
            dim_level_milli: AtomicUsize::new(
                (settings.features.dim_level * 1000.0) as usize
            ),
            hwnd,
            tray_nid: NOTIFYICONDATAW::default(),
        }));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        let state = &mut *state_ptr;

        state.apply_mode();

        // Tray
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = hicon;
        update_tray_tip(&mut nid, &format!("Invertonator — {}", state.mode_text()));
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        state.tray_nid = nid;

        // Hotkeys
        if !register_hotkeys(hwnd) {
            tracing::warn!("some hotkeys failed — tray + quit hotkey may be unavailable");
        }

        // Message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        let _ = Shell_NotifyIconW(NIM_DELETE, &state.tray_nid);
        magnification::uninitialize();
        tracing::info!("clean exit — colors restored");
    }
}