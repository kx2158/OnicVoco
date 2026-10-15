// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod analyzer;
mod app;
mod audio;
mod content;
mod dsp;
mod profile;
mod session;
mod storage;
mod theme;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let icon = egui::IconData {
        rgba: include_bytes!("../assets/icon64.rgba").to_vec(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("OnicVoco")
            .with_inner_size([1180.0, 880.0])
            .with_min_inner_size([960.0, 640.0])
            .with_icon(std::sync::Arc::new(icon)),
        centered: true,
        ..Default::default()
    };
    eframe::run_native("OnicVoco", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
