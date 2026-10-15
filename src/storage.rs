//! Settings and progress history, saved as JSON in the user's app data folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    /// "system", "light" or "dark"
    pub theme: String,
    pub goal: String,
    pub custom_min: f32,
    pub custom_max: f32,
    pub gate_db: f32,
    pub record: bool,
    /// Keep the mic open (live pitch + peak meter) even when not recording.
    pub live_mic: bool,
    pub mic: Option<String>,
    pub ai_enabled: bool,
    pub passaggio_hz: Option<f32>,
    pub passage: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "system".into(),
            goal: "feminine".into(),
            custom_min: 160.0,
            custom_max: 220.0,
            gate_db: -50.0,
            record: true,
            live_mic: true,
            mic: None,
            ai_enabled: false,
            passaggio_hz: None,
            passage: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HistoryEntry {
    /// Unix seconds
    pub at: i64,
    pub goal: String,
    pub median_hz: f32,
    pub note: String,
    pub in_target: f32,
    pub resonance: Option<f32>,
    pub seconds: f32,
    #[serde(default)]
    pub cracks: usize,
    #[serde(default)]
    pub registers: Option<(f32, f32, f32)>,
}

pub fn data_dir() -> PathBuf {
    let d = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("OnicVoco");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Large files (the AI model) go in the local, non-roaming folder.
pub fn local_dir() -> PathBuf {
    let d = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("OnicVoco");
    let _ = std::fs::create_dir_all(&d);
    d
}

fn read<T: for<'de> Deserialize<'de>>(name: &str) -> Option<T> {
    let s = std::fs::read_to_string(data_dir().join(name)).ok()?;
    serde_json::from_str(&s).ok()
}

fn write<T: Serialize>(name: &str, v: &T) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let path = data_dir().join(name);
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, s).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

pub fn load_settings() -> Settings {
    read("settings.json").unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    write("settings.json", s);
}

pub fn load_history() -> Vec<HistoryEntry> {
    read("history.json").unwrap_or_default()
}

pub fn save_history(h: &[HistoryEntry]) {
    write("history.json", &h);
}

pub fn load_profile() -> crate::profile::Profile {
    let mut p: crate::profile::Profile = read("profile.json").unwrap_or_default();
    p.prepare();
    p
}

pub fn save_profile(p: &crate::profile::Profile) {
    write("profile.json", p);
}
