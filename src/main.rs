// Invertonator-rs — v0.2.0-stepA: UIA probe (diagnostic build)
//
//   Milestone 1: magnification invert/Dim, tray, hotkeys, settings. DONE.
//   Milestone 2 STEP A: prove COM + IUIAutomation + cache-request walks.
//
//   NEW DIAGNOSTICS:
//     · probe logs the ROOT WINDOW NAME — answers "what are we probing?"
//     · walk_live logs every visited node + children count — answers
//       "is the walk descending or stopping at depth 1?"
//
//   Press Ctrl+Alt+F10 with OPERA focused (not the terminal!).

#![windows_subsystem = "windows"]
// ^ uncomment for release; keep commented while debugging so panics/logs
//   print to the console.

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
    GetForegroundWindow,
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
// Magnification engine (Milestone 1)
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
// RAW CreateWindowExW (ledger: "failed successfully" quirk)
// =====================================================================

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

// =====================================================================
// UIA PROBE — Milestone 2 Step A (with diagnostics)
// =====================================================================

mod uia {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope,
        TreeScope_Children, TreeScope_Descendants,
        UIA_BoundingRectanglePropertyId, UIA_ControlTypePropertyId,
        UIA_LocalizedControlTypePropertyId, UIA_NamePropertyId,
    };

    /// Probe the given window: returns (total_nodes, media_count, samples).
    pub fn probe(hwnd: HWND) -> (u32, u32, Vec<String>) {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() {
                tracing::info!("uia-probe: CoInitializeEx: {hr} (ok if already init)");
            }

            let automation: IUIAutomation =
                match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::error!("uia-probe: CoCreateInstance failed: {e}");
                        return (0, 0, Vec::new());
                    }
                };

            let Ok(root) = automation.ElementFromHandle(hwnd) else {
                tracing::error!("uia-probe: ElementFromHandle failed");
                return (0, 0, Vec::new());
            };
            // WARM-UP: poke Chromium to fully expand the AX tree (the C#
            // tracker's first FindAll did this implicitly; explicit is better).
            let Ok(true_cond_warm) = automation.CreateTrueCondition() else {
                tracing::error!("uia-probe: warmup condition failed");
                return (0, 0, Vec::new());
            };
            match root.FindAll(TreeScope_Descendants, &true_cond_warm) {
                Ok(pre) => {
                    let n = pre.Length().unwrap_or(0);
                    tracing::info!("uia-probe: warm-up FindAll: {n} elements");
                }
                Err(e) => tracing::warn!("uia-probe: warm-up failed: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(800));

            // DIAGNOSTIC: which window are we actually probing?
            let root_name = root
                .CurrentName()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "<unnamed>".into());
            tracing::info!("uia-probe: probing window \"{root_name}\"");

            // Cache request: all four properties, whole subtree, one call.
            let Ok(cache) = automation.CreateCacheRequest() else {
                tracing::error!("uia-probe: CreateCacheRequest failed");
                return (0, 0, Vec::new());
            };
            let _ = cache.AddProperty(UIA_ControlTypePropertyId);
            let _ = cache.AddProperty(UIA_NamePropertyId);
            let _ = cache.AddProperty(UIA_LocalizedControlTypePropertyId);
            let _ = cache.AddProperty(UIA_ControlTypePropertyId);
            let _ = cache.AddProperty(UIA_NamePropertyId);
            let _ = cache.AddProperty(UIA_LocalizedControlTypePropertyId);
            let _ = cache.AddProperty(UIA_BoundingRectanglePropertyId);
            // ORDER MATTERS: filter BEFORE scope (COM docs order). And the
            // earlier BuildUpdatedCache failure was WITH these two calls but
            // WITHOUT... unknown — the fallback below covers any recurrence,
            // and the cache path is worth one more attempt now that the
            // environment is proven healthy (warm-up saw 931 nodes).
            let Ok(true_cond) = automation.CreateTrueCondition() else {
                tracing::error!("uia-probe: CreateTrueCondition failed");
                return (0, 0, Vec::new());
            };
            let _ = cache.SetTreeFilter(&true_cond);
            let _ = cache.SetTreeScope(TreeScope_Descendants);

            let cached_root = match root.BuildUpdatedCache(&cache) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(
                        "uia-probe: BuildUpdatedCache failed ({e}) — falling back to live walk"
                    );
                    // FALLBACK: live walk (proven C# technology).
                    let Ok(true_cond) = automation.CreateTrueCondition() else {
                        tracing::error!("uia-probe: CreateTrueCondition failed");
                        return (0, 0, Vec::new());
                    };
                    let Ok(all) = root.FindAll(TreeScope_Descendants, &true_cond) else {
                        tracing::error!("uia-probe: FindAll failed");
                        return (0, 0, Vec::new());
                    };
                    let Ok(len) = all.Length() else { return (0, 0, Vec::new()); };
                    tracing::info!("uia-probe fallback: FindAll returned {len} elements");
                    let mut total: u32 = 0;
                    let mut media: u32 = 0;
                    let mut samples: Vec<String> = Vec::new();
                    for i in 0..len {
                        if let Ok(el) = all.GetElement(i) {
                            walk_live(el, &true_cond, &mut total, &mut media, &mut samples);
                        }
                    }
                    return (total, media, samples);
                }
            };

            // In-process walk of cached data.
            let mut total: u32 = 0;
            let mut media: u32 = 0;
            let mut samples: Vec<String> = Vec::new();
            walk(cached_root, 0, &mut total, &mut media, &mut samples);

            (total, media, samples)
        }
    }

    fn walk(
        el: IUIAutomationElement,
        depth: u32,
        total: &mut u32,
        media: &mut u32,
        samples: &mut Vec<String>,
    ) {
        unsafe {
            if *total > 100_000 || depth > 25 {
                return;
            }
            *total += 1;

            let lct = el
                .CachedLocalizedControlType()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let name = el
                .CachedName()
                .map(|s| s.to_string())
                .unwrap_or_default();

            let lct_lower = lct.to_lowercase();
            if matches!(lct_lower.as_str(), "image" | "graphic" | "link" | "video") {
                *media += 1;
                if samples.len() < 8 {
                    let rect = el.CachedBoundingRectangle().unwrap_or_default();
                    let n = if name.len() > 30 { name[..30].to_string() } else { name };
                    samples.push(format!(
                        "[{lct}] \"{n}\" {}x{} @({},{})",
                        rect.right - rect.left, rect.bottom - rect.top,
                        rect.left, rect.top
                    ));
                }
            }

            if let Ok(kids) = el.GetCachedChildren() {
                if let Ok(len) = kids.Length() {
                    for i in 0..len {
                        if let Ok(kid) = kids.GetElement(i) {
                            walk(kid, depth + 1, total, media, samples);
                        }
                    }
                }
            }
        }
    }

    /// Fallback walk with LIVE elements — logs every step (diagnostic).
    fn walk_live(
        el: IUIAutomationElement,
        true_cond: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
        total: &mut u32,
        media: &mut u32,
        samples: &mut Vec<String>,
    ) {
        unsafe {
            *total += 1;

            let lct = el
                .CurrentLocalizedControlType()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let name = el.CurrentName().map(|s| s.to_string()).unwrap_or_default();
            let rect = el.CurrentBoundingRectangle().unwrap_or_default();

            tracing::info!(
                "walk_live: [{}] \"{}\" {}x{} @({},{})",
                lct, name, rect.right - rect.left, rect.bottom - rect.top,
                rect.left, rect.top
            );

            let lct_lower = lct.to_lowercase();
            if matches!(lct_lower.as_str(), "image" | "graphic" | "link" | "video") {
                *media += 1;
                if samples.len() < 8 {
                    let n = if name.len() > 30 { name[..30].to_string() } else { name };
                    samples.push(format!(
                        "[{lct}] \"{n}\" {}x{} @({},{})",
                        rect.right - rect.left, rect.bottom - rect.top,
                        rect.left, rect.top
                    ));
                }
            }

            // DIAGNOSTIC: how many children does FindAll see here?
            match el.FindAll(TreeScope_Children, true_cond) {
                Ok(kids) => {
                    let len = kids.Length().unwrap_or(0);
                    tracing::info!("walk_live: children={len}");
                    for i in 0..len {
                        if let Ok(kid) = kids.GetElement(i) {
                            walk_live(kid, true_cond, total, media, samples);
                        }
                    }
                }
                Err(e) => {
                    tracing::info!("walk_live: FindAll(children) failed: {e}");
                }
            }
        }
    }
}

// =====================================================================
// Application state
// =====================================================================

use magnification::ScreenMode;

struct AppState {
    mode: ScreenMode,
    invert_on: AtomicBool,
    dim_level_milli: AtomicUsize,
    // Used in Milestone 3 (hole placement relative to the host window).
    #[allow(dead_code)]
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

const WM_TRAYICON: u32 = 0x8000;
const WM_LBUTTONUP_TRAY: usize = 0x0202;

static TOGGLE_REQUEST: AtomicBool = AtomicBool::new(false);
static QUIT_REQUEST: AtomicBool = AtomicBool::new(false);
static CYCLE_REQUEST: AtomicBool = AtomicBool::new(false);
static PROBE_REQUEST: AtomicBool = AtomicBool::new(false);

const HOTKEY_TOGGLE: i32 = 1;
const HOTKEY_QUIT: i32 = 2;
const HOTKEY_CYCLE: i32 = 3;
const HOTKEY_PROBE: i32 = 4;
const HOTKEY_MODS: HOT_KEY_MODIFIERS = HOT_KEY_MODIFIERS(0x1 | 0x2 | 0x4000);
const VK_F5: u32 = 0x74;
const VK_F2: u32 = 0x71;
const VK_F6: u32 = 0x75;
const VK_F10: u32 = 0x79;

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
    let d = register(HOTKEY_PROBE, VK_F10, "uia-probe");
    a && b && c && d
}

/// SAFE tray-tip write (the [u16;128] panic fix).
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
                    HOTKEY_PROBE => PROBE_REQUEST.store(true, Ordering::Relaxed),
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
        if PROBE_REQUEST.swap(false, Ordering::Relaxed) {
            // Probe targets the FOREGROUND window. NOTE: press F10 with
            // OPERA focused — pressing it while the terminal is focused
            // probes the terminal instead (the total=1 mystery).
            let target = GetForegroundWindow();
            let (total, media, samples) = uia::probe(target);
            tracing::info!("uia-probe: total={total} media={media}");
            for s in &samples {
                tracing::info!("  media sample: {s}");
            }
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

    tracing::info!("=== Invertonator-rs v0.2.0-stepA (UIA probe, diagnostic) ===");

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

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            windows::core::w!("Invertonator").as_ptr(),
            0,
            CW_USEDEFAULT, CW_USEDEFAULT,
            CW_USEDEFAULT, CW_USEDEFAULT,
            HWND::default(),
            0,
            hinstance,
            std::ptr::null_mut(),
        );
        if hwnd.is_invalid() {
            let err = std::io::Error::last_os_error();
            tracing::error!("CreateWindowExW failed: {err}");
            return;
        }

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

        if !register_hotkeys(hwnd) {
            tracing::warn!("some hotkeys failed");
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &state.tray_nid);
        magnification::uninitialize();
        tracing::info!("clean exit — colors restored");
    }
}