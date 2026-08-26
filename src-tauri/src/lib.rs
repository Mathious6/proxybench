mod commands;
mod country;
mod export;
mod import;
mod inventory;
mod last_target;
mod parse;
mod probe;
mod run;
mod session;
mod split;
mod stats;
mod tags;
mod target;

use commands::LastTarget;
use import::{InventoryStore, TagStore};
use session::{Session, SessionStore};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let store = import::open_store(app.handle())?;
            let inventory = import::open_inventory(app.handle())?;
            let last = commands::open_last_target(app.handle())?;
            let session = Session::restore(inventory.load()?);
            app.manage(TagStore(std::sync::Mutex::new(store)));
            app.manage(SessionStore(std::sync::Mutex::new(session)));
            app.manage(InventoryStore(inventory));
            app.manage(LastTarget(last));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            import::import_paths,
            import::set_tags,
            commands::last_target,
            commands::start_run,
            commands::export_dir,
            commands::session_rows,
            commands::remove_subnet
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
