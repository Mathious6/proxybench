mod archive;
mod aycd;
mod cancel;
mod clock;
mod commands;
mod country;
mod dns;
mod export;
mod health;
mod import;
mod inventory;
mod inventory_file;
mod library;
mod measurement;
mod probe;
mod proxy;
mod run;
mod sample;
mod secure_file;
mod settings;
mod slot;
mod state;
mod stats;
mod subnet;
mod tag;
mod target;
mod wire;
mod workflow;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(state::AppState::open(&dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::rows,
            commands::import_paths,
            commands::set_tags,
            commands::remove,
            commands::settings,
            commands::save_settings,
            commands::check,
            commands::probe,
            commands::stop,
            commands::export_txt,
            commands::export_aycd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
