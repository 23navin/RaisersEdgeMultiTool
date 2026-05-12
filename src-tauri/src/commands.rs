// commands.rs
//
// All three invoke() targets the frontend calls.
// Thin layer — receives args, calls profile.rs or db.rs, returns result.
// Errors are converted to String so the frontend receives them directly.

use std::path::Path;
use crate::profile::{self, ProfileSummary, LoadedProfile, NoticeQuery};
use crate::db::{self, ValidationResult, TransformResult, NoticeInput};
use crate::errors::AppError;

// ── list_profiles ─────────────────────────────────────────────────────────────
// Called by: App.tsx on mount
// Scans profiles/ folder, returns lightweight summary for each .import file

#[tauri::command]
pub fn list_profiles() -> Result<Vec<ProfileSummary>, String> {
    #[cfg(debug_assertions)]
    let profiles_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "cannot find project root".to_string())?
        .join("profiles");

    #[cfg(not(debug_assertions))]
    let profiles_dir = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or_else(|| "cannot find exe dir".to_string())?
        .join("profiles");

    profile::list_profiles(&profiles_dir).map_err(|e| e.to_string())
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
    sql_file: String,             // filename e.g. "primary_transform.sql"
    zip_path: String,             // temp dir path — profile already extracted
    output_labels: Vec<String>,   // every output declared on this transform
) -> Result<TransformResult, String> {
    let loaded = profile::load_from_dir(Path::new(&zip_path))
        .map_err(|e| e.to_string())?;

    let sql = loaded.sql_files.get(&sql_file)
        .ok_or_else(|| format!("SQL file '{}' not found in profile", sql_file))?;

    // Find the transform that owns this sql_file so we can pick up any
    // notice queries it declares. Matches by sql filename — adequate while
    // each transform within a profile names a unique .sql file.
    let notice_defs = find_notices_for_sql(&loaded, &sql_file);
    let notices: Vec<NoticeInput> = notice_defs
        .iter()
        .filter_map(|n| {
            loaded.sql_files.get(&n.sql).map(|content| NoticeInput {
                label: &n.label,
                description: n.description.as_deref(),
                sql_content: content,
            })
        })
        .collect();

    db::run_transform(Path::new(&file_path), sql, &output_labels, &notices)
        .map_err(|e| e.to_string())
}

// Returns the NoticeQuery list attached to the first transform whose `sql`
// field matches `sql_file`. Walks both the multi-transform `transforms` form
// and the legacy single-transform shortcut on the step itself.
fn find_notices_for_sql<'a>(loaded: &'a LoadedProfile, sql_file: &str) -> Vec<&'a NoticeQuery> {
    for step in &loaded.structure.steps {
        if step.step_type != "sql_transform" {
            continue;
        }
        if let Some(transforms) = &step.transforms {
            for t in transforms {
                if t.sql == sql_file {
                    return t.notices.as_deref().unwrap_or(&[]).iter().collect();
                }
            }
        }
        if step.sql.as_deref() == Some(sql_file) {
            return step.notices.as_deref().unwrap_or(&[]).iter().collect();
        }
    }
    Vec::new()
}

// ── save_output ───────────────────────────────────────────────────────────────
// Called by: StepPanel.tsx when user confirms the Save As dialog
// Copies the generated temp file to the user-chosen destination

#[tauri::command]
pub fn save_output(src_path: String, dest_path: String) -> Result<(), String> {
    std::fs::copy(&src_path, &dest_path)
        .map(|_| ())
        .map_err(|e| e.to_string())
}