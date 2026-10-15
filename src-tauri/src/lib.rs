#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|_app| {
            #[cfg(target_os = "linux")]
            enable_linux_microphone(_app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running OnicVoco");
}

/// WebKitGTK ships with getUserMedia disabled and denies permission requests
/// by default. OnicVoco only ever loads its own bundled UI, so we enable media
/// streams and grant the microphone request.
#[cfg(target_os = "linux")]
fn enable_linux_microphone(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::Manager;
    use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};

    if let Some(window) = app.get_webview_window("main") {
        window.with_webview(|webview| {
            let view = webview.inner();
            if let Some(settings) = WebViewExt::settings(&view) {
                settings.set_enable_media_stream(true);
            }
            view.connect_permission_request(|_, request| {
                request.allow();
                true
            });
        })?;
    }
    Ok(())
}
