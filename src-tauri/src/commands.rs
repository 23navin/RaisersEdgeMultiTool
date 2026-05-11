// commands.rs
//
// Tauri commands — these are the functions the frontend calls via invoke().
// Each function here is a direct bridge: receive args from frontend,
// call into profile.rs or db.rs, return result or error string.
//
// Rules:
//   - Every function must be registered in main.rs to work
//   - Errors must be converted to String — the frontend receives them as-is
//   - Args come in as basic types (String, bool) — no complex types from frontend

use std::path::Path;
use crate::profile;
use crate::profile::{ProfileSummary, LoadedProfile};
use crate::errors::AppError;

// ── list_profiles ─────────────────────────────────────────────────────────────
// Called by: ProfilePicker.tsx on mount
// Returns: array of ProfileSummary (id, name, version, zip_path)
// The frontend uses this to populate the dropdown — lightweight, no full parse

#[tauri::command]
pub fn list_profiles(profiles_dir: String) -> Result<Vec<ProfileSummary>, String> {
    profile::list_profiles(Path::new(&profiles_dir))
        .map_err(|e| e.to_string())
}

// ── load_profile ──────────────────────────────────────────────────────────────
// Called by: ProfilePicker.tsx when user selects a profile
// Returns: full LoadedProfile (structure, instructions, sql_files)
// Frontend uses this to render the step list and instruction panels

#[tauri::command]
pub fn load_profile(zip_path: String) -> Result<LoadedProfile, String> {
    profile::load_profile(Path::new(&zip_path))
        .map_err(|e| e.to_string())
}