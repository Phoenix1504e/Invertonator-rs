// Invertonator-rs — common: code shared by every platform backend.
//
// Settings keep the same settings.json format as the C# edition (the
// file lives next to the binary on every OS). Logging always goes to
// the OS-local app-data dir: %LOCALAPPDATA%\Invertonator\log.txt on
// Windows, ~/.local/share/Invertonator/log.txt on Linux.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const VERSION: &str = "0.3.0-alpha.3";

// =====================================================================
// Settings — same settings.json format as the C# edition.
// =====================================================================

// Linux v1 does not consume settings.json yet (its hotkeys are
// hardcoded Ctrl+Alt+I/Q); the structs stay compiled for BOTH platforms
// so the file format remains identical everywhere — Linux v2 wires
// them up. Hence the targeted dead_code allowances.
#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    pub hotkeys: HotkeySettings,
    pub features: FeatureSettings,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct HotkeySettings {
    pub invert: String,
    pub mode: String,
    pub pierce: String,
    pub quit: String,
    pub probe: String,
    pub magprobe: String,
}

impl Default for HotkeySettings {
    fn default() -> Self {
        // THE SOURCE OF TRUTH IS the hotkey registration code on each
        // platform — these defaults document the real bindings (the old
        // C#-edition values here were aspirational: they said F8=quit
        // while the binary has always quit on Ctrl+Alt+F2).
        Self {
            invert: "Ctrl+Alt+F5".into(),
            mode: "Ctrl+Alt+F6".into(),
            pierce: "Ctrl+Alt+F7".into(),
            quit: "Ctrl+Alt+F2".into(),
            probe: "Ctrl+Alt+F10".into(),
            magprobe: "Ctrl+Alt+F11".into(),
        }
    }
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct FeatureSettings {
    pub at_mode: bool,
    pub dim_level: f64,
}

impl Default for FeatureSettings {
    fn default() -> Self {
        Self { at_mode: false, dim_level: 0.4 }
    }
}

#[allow(dead_code)]
impl Settings {
    fn path() -> PathBuf {
        let exe = std::env::current_exe().unwrap_or_default();
        exe.parent()
            .map(|p| p.join("settings.json"))
            .unwrap_or_else(|| PathBuf::from("settings.json"))
    }

    pub fn load() -> Self {
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

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::path(), text);
            tracing::info!("settings: saved");
        }
    }
}

// =====================================================================
// Logging — same location scheme on every OS via `dirs`.
// =====================================================================

pub fn log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_default()
        .join("Invertonator")
}

pub fn init_logging() {
    let dir = log_dir();
    std::fs::create_dir_all(&dir).ok();
    let log_file = tracing_appender::rolling::never(dir, "log.txt");
    tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_ansi(false)
        .init();
}
