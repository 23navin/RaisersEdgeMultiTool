// commands.rs
//
// All three invoke() targets the frontend calls.
// Thin layer — receives args, calls profile.rs or db.rs, returns result.
// Errors are converted to String so the frontend receives them directly.

use std::path::Path;
use crate::profile::{self, ProfileSummary, LoadedProfile};
use crate::db::{self, ValidationResult, TransformResult};
use crate::errors::AppError;

// ── list_profiles ─────────────────────────────────────────────────────────────
// Called by: App.tsx on mount
// Scans profiles/ folder, returns lightweight summary for each .import file

#[tauri::command]
pub fn list_profiles(profiles_dir: String) -> Result<Vec<ProfileSummary>, String> {
    profile::list_profiles(Path::new(&profiles_dir))
        .map_err(|e| e.to_string())
}

// ── load_profile ──────────────────────────────────────────────────────────────
// Called by: App.tsx when user selects a profile from dropdown
// Fully parses the bundle — structure, instructions, sql files

#[tauri::command]
pub fn load_profile(zip_path: String) -> Result<LoadedProfile, String> {
    profile::load_profile(Path::new(&zip_path))
        .map_err(|e| e.to_string())
}

// ── validate_file ─────────────────────────────────────────────────────────────
// Called by: StepPanel.tsx on validation steps
// Checks a file's columns against the profile's validation rules for that input

#[tauri::command]
pub fn validate_file(
    file_path: String,
    profile_id: String,      // used for error messages
    input_label: String,     // which input definition to validate against
    zip_path: String,        // path to extracted temp dir to re-read the profile
) -> Result<ValidationResult, String> {
    // Re-load the profile to get the validation rules
    // (profile is already extracted, temp_dir path comes from frontend)
    let loaded = profile::load_from_dir(Path::new(&zip_path))
        .map_err(|e| e.to_string())?;

    // Find the input definition for this label
    let input_def = loaded.structure.inputs
        .iter()
        .find(|i| i.label == input_label)
        .ok_or_else(|| format!("No input definition found for '{}'", input_label))?;

    let validations = input_def.validation.as_deref().unwrap_or(&[]);

    db::validate_file(Path::new(&file_path), validations)
        .map_err(|e| e.to_string())
}

// ── run_profile ───────────────────────────────────────────────────────────────
// Called by: StepPanel.tsx on sql_transform steps
// Executes the step's SQL against the attached input file, writes output CSV

#[tauri::command]
pub fn run_profile(
    file_path: String,
    sql_file: String,        // filename e.g. "primary_transform.sql"
    zip_path: String,        // temp dir path — profile already extracted
    output_label: String,    // used for output filename prefix
) -> Result<TransformResult, String> {
    let loaded = profile::load_from_dir(Path::new(&zip_path))
        .map_err(|e| e.to_string())?;

    let sql = loaded.sql_files.get(&sql_file)
        .ok_or_else(|| format!("SQL file '{}' not found in profile", sql_file))?;

    db::run_transform(Path::new(&file_path), sql, &output_label)
        .map_err(|e| e.to_string())
}