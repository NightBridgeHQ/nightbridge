fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            lsi_gui::gui_load_settings,
            lsi_gui::gui_save_settings
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LocalSend Improved GUI");
}
