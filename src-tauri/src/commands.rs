// commands.rs
//
// The Tauri shell: one #[tauri::command] wrapper per core::api operation.
// Each wrapper builds a Ctx from the AppHandle (where state lives + how to
// reach RE via sky_auth), hops onto a blocking thread when the body does
// blocking I/O, and converts AppError to the String the frontend expects.
// All real logic lives in multitool_core::api.

use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use multitool_core::api::{self, Ctx, ProfileMutation};
use multitool_core::code_tables;
use multitool_core::db::{self, ValidationResult, TransformResult};
use multitool_core::errors::AppError;
use multitool_core::profile::{ProfileSummary, LoadedProfile, ProfileFileEntry};
use multitool_core::query_step;
use multitool_core::re_calls::Transport;
use multitool_core::user_input;
use multitool_core::report::{ReportRunResult, ActionResult};
use multitool_core::validate::ValidationReport;
use crate::sky_auth;

// Where session workspaces live on desktop. Under the OS temp dir — the
// session reaper (core::workspace) clears stale ones, and the OS reclaims the
// rest.
fn workspaces_root() -> PathBuf {
    std::env::temp_dir().join("multitool-sessions")
}

// Pick the RE transport for a run. Live when connected to RE NXT, unless
// RE_NXT_MOCK is set (forces the fixture path for offline dev). Blocking —
// the Ctx closure is only ever called from a blocking thread.
fn resolve_transport(app: &AppHandle) -> Result<Transport, AppError> {
    let force_mock = std::env::var("RE_NXT_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !force_mock && sky_auth::has_connection(app) {
        let (access_token, subscription_key) = sky_auth::live_credentials(app)?;
        Ok(Transport::Live { access_token, subscription_key })
    } else {
        Ok(Transport::Mock)
    }
}

fn ctx(app: &AppHandle) -> Result<Ctx, String> {
    let user_profiles_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    let app = app.clone();
    Ok(Ctx {
        workspaces_root: workspaces_root(),
        user_profiles_dir,
        transport: Box::new(move || resolve_transport(&app)),
    })
}

// ── profiles ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileSummary>, String> {
    api::list_profiles(&ctx(&app)?).map_err(|e| e.to_string())
}

// `zipPath` is the profile ref ("builtin://<file>" or "user://<file>") — the
// arg name is kept for wire compatibility with ProfileSummary.zip_path.
#[tauri::command]
pub fn load_profile(app: AppHandle, zip_path: String) -> Result<LoadedProfile, String> {
    api::load_profile(&ctx(&app)?, &zip_path).map_err(|e| e.to_string())
}

// ── import pipeline ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn validate_file(
    app: AppHandle,
    file_path: String,
    input_label: String,
    session_id: String,
) -> Result<ValidationResult, String> {
    api::validate_file(&ctx(&app)?, &file_path, &input_label, &session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn run_profile(
    app: AppHandle,
    file_paths: HashMap<String, String>,  // input_label → local file path
    query_ids: HashMap<String, String>,   // query_output label → artifact id
    sync_ids: HashMap<String, String>,    // sync_output label → artifact id
    form_ids: HashMap<String, String>,    // form_output label → artifact id
    sql_file: String,
    session_id: String,
    output_labels: Vec<String>,
) -> Result<TransformResult, String> {
    let ctx = ctx(&app)?;
    // Off the async runtime — pulling code tables does blocking network I/O
    // (reqwest::blocking panics inside a Tokio context).
    tokio::task::spawn_blocking(move || {
        api::run_profile(
            &ctx,
            file_paths,
            query_ids,
            sync_ids,
            form_ids,
            &sql_file,
            &session_id,
            &output_labels,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn run_re_query(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    step_label: String,
    session_id: String,
) -> Result<query_step::QueryStepResult, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_re_query(&ctx, file_paths, &step_label, &session_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn run_code_table_sync(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    step_label: String,
    session_id: String,
) -> Result<code_tables::SyncResult, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_code_table_sync(&ctx, file_paths, &step_label, &session_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn run_visualization(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    form_ids: HashMap<String, String>,
    step_label: String,
    session_id: String,
) -> Result<db::ResultSet, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_visualization(
            &ctx,
            file_paths,
            query_ids,
            sync_ids,
            form_ids,
            &step_label,
            &session_id,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// Recomputes a user_input step's rows and republishes the values the user has
// typed. Called on every edit, so it stays off the network unless the profile
// declares code tables.
#[tauri::command]
pub async fn run_user_input(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    step_label: String,
    session_id: String,
    values: HashMap<String, HashMap<String, String>>, // row key → field id → value
) -> Result<user_input::UserInputResult, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_user_input(
            &ctx,
            file_paths,
            query_ids,
            sync_ids,
            &step_label,
            &session_id,
            &values,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── reports ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn run_report(
    app: AppHandle,
    session_id: String,
    param_values: HashMap<String, serde_json::Value>,
) -> Result<ReportRunResult, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_report(&ctx, &session_id, &param_values).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn run_report_action(
    app: AppHandle,
    session_id: String,
    action_id: String,
    param_values: HashMap<String, serde_json::Value>,
) -> Result<ActionResult, String> {
    let ctx = ctx(&app)?;
    tokio::task::spawn_blocking(move || {
        api::run_report_action(&ctx, &session_id, &action_id, &param_values)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── save_output ───────────────────────────────────────────────────────────────
// Desktop-only Save As: resolve the artifact inside its session, then copy to
// the destination the native save dialog produced. The web shell replaces
// this with a download endpoint.

#[tauri::command]
pub fn save_output(
    app: AppHandle,
    session_id: String,
    artifact_id: String,
    dest_path: String,
) -> Result<(), String> {
    let src = api::resolve_artifact(&ctx(&app)?, &session_id, &artifact_id)
        .map_err(|e| e.to_string())?;
    std::fs::copy(&src, &dest_path)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ── profile editor ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn save_profile(
    app: AppHandle,
    zip_path: String,
    files: Vec<ProfileFileEntry>,
) -> Result<ProfileMutation, String> {
    api::save_profile(&ctx(&app)?, &zip_path, &files).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn new_profile(app: AppHandle) -> Result<ProfileMutation, String> {
    api::new_profile(&ctx(&app)?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_profile(
    app: AppHandle,
    source_zip_path: String,
) -> Result<ProfileMutation, String> {
    api::duplicate_profile(&ctx(&app)?, &source_zip_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_profile(app: AppHandle, zip_path: String) -> Result<(), String> {
    api::delete_profile(&ctx(&app)?, &zip_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn validate_profile(files: Vec<ProfileFileEntry>) -> Result<ValidationReport, String> {
    Ok(api::validate_profile(&files))
}

#[tauri::command]
pub fn scaffold_missing(files: Vec<ProfileFileEntry>) -> Result<Vec<ProfileFileEntry>, String> {
    api::scaffold_missing(&files).map_err(|e| e.to_string())
}
