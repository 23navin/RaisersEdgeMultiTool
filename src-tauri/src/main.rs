// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod errors;
mod profile;
mod db;
mod commands;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::load_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}