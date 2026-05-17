fn main() {
    tauri::Builder::default()
        .manage(std::sync::Mutex::new(lsi_gui::EmbeddedDaemonManager::default()))
        .invoke_handler(tauri::generate_handler![
            lsi_gui::gui_embedded_daemon_status,
            lsi_gui::gui_load_settings,
            lsi_gui::gui_save_settings,
            lsi_gui::gui_start_embedded_daemon,
            lsi_gui::gui_stop_embedded_daemon
        ])
        .run(tauri::generate_context!())
        .expect("failed to run NightBridge GUI");
}
