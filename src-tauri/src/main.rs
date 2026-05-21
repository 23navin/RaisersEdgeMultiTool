// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod errors;
mod profile;
mod db;
mod validate;
mod commands;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::load_profile,
            commands::validate_file,
            commands::run_profile,
            commands::save_output,
            commands::save_profile,
            commands::new_profile,
            commands::duplicate_profile,
            commands::delete_profile,
            commands::validate_profile,
            commands::scaffold_missing,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}