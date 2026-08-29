// commands.rs
//
// All five invoke() targets the frontend calls.
// Thin layer — receives args, calls profile.rs or db.rs, returns result.
// Errors are converted to String so the frontend receives them directly.

use std::collections::HashMap;
use std::path::Path;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use crate::profile::{self, ProfileSummary, LoadedProfile, NoticeQuery, ProfileFileEntry};
use crate::db::{self, ValidationResult, TransformResult, NoticeInput};
use crate::report::{self, ReportRunResult, ActionResult};
use crate::code_tables;
use crate::query_step;
use crate::re_calls::Transport;
use crate::sky_auth;
use crate::validate::{self, ValidationReport};

// Combined return for create / duplicate / save — the frontend wants both
// the new sidebar summary and the freshly extracted contents in one round-trip.
#[derive(Serialize)]
pub struct ProfileMutation {
    pub summary: ProfileSummary,
    pub loaded: LoadedProfile,
}

fn user_profiles_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

// ── list_profiles ─────────────────────────────────────────────────────────────
// Called by: App.tsx on mount
// Returns built-in profiles (embedded in the binary) plus any user .import
// files found in the per-user profiles directory under app_data_dir.

#[tauri::command]
pub fn list_profiles(app: AppHandle) -> Result<Vec<ProfileSummary>, String> {
    let user_dir = app.path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("profiles");
    // Best-effort: ensure the dir exists so the user can drop files there
    // without having to mkdir it themselves. Don't fail listing if it can't.
    let _ = std::fs::create_dir_all(&user_dir);

    let mut out = profile::list_builtin_profiles().map_err(|e| e.to_string())?;
    out.extend(profile::list_user_profiles(&user_dir).map_err(|e| e.to_string())?);
    Ok(out)
}

// ── load_profile ──────────────────────────────────────────────────────────────
// Called by: App.tsx when user selects a profile from dropdown
// Fully parses the bundle — structure, instructions, sql files.
// A `builtin://<filename>` zip_path resolves to an embedded profile.

#[tauri::command]
pub fn load_profile(zip_path: String) -> Result<LoadedProfile, String> {
    if let Some(name) = zip_path.strip_prefix("builtin://") {
        return profile::load_builtin(name).map_err(|e| e.to_string());
    }
    profile::load_profile(Path::new(&zip_path))
        .map_err(|e| e.to_string())
}

// ── validate_file ─────────────────────────────────────────────────────────────
// Called by: StepPanel.tsx on validation steps
// Checks a file's columns against the profile's validation rules for that input

#[tauri::command]
pub fn validate_file(
    file_path: String,
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
// Executes the step's SQL against the attached input files, writes output CSV.
//
// `file_paths` maps each declared input label to its uploaded file path.
// The SQL author references inputs with {{input:Label}} placeholders, or
// {{input_file}} as a legacy single-input alias (only valid when the
// transform has exactly one input).

#[tauri::command]
pub async fn run_profile(
    app: AppHandle,
    file_paths: HashMap<String, String>,  // input_label → file_path
    query_paths: HashMap<String, String>, // query_output label → result JSON path
    sync_paths: HashMap<String, String>,  // sync_output label → outcome JSON path
    sql_file: String,                     // filename e.g. "primary_transform.sql"
    zip_path: String,                     // temp dir path — profile already extracted
    output_labels: Vec<String>,           // every output declared on this transform
) -> Result<TransformResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        run_profile_blocking(
            &app,
            file_paths,
            query_paths,
            sync_paths,
            sql_file,
            zip_path,
            output_labels,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

// The body of run_profile, off the async runtime — pulling code tables does
// blocking network I/O (reqwest::blocking panics inside a Tokio context).
fn run_profile_blocking(
    app: &AppHandle,
    file_paths: HashMap<String, String>,
    query_paths: HashMap<String, String>,
    sync_paths: HashMap<String, String>,
    sql_file: String,
    zip_path: String,
    output_labels: Vec<String>,
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

    // Pull any code tables the profile declares so {{codetable:Label}} resolves.
    // No-ops (and costs nothing) for the profiles that declare none.
    let code_table_paths = if loaded.structure.code_tables.is_empty() {
        HashMap::new()
    } else {
        let transport = resolve_transport(app)?;
        let run_dir = std::env::temp_dir().join(format!("codetables-{}", loaded.structure.id));
        code_tables::fetch_all(&loaded, &transport, &run_dir).map_err(|e| e.to_string())?
    };

    // Everything the SQL body may reference beyond {{input:}} / {{output:}}.
    // One registry, so a new placeholder family is a line here rather than a
    // new parameter on run_transform.
    let sources = db::SqlSources::new()
        .with(db::KIND_CODETABLE, &code_table_paths)
        .with(db::KIND_QUERY, &query_paths)
        .with(db::KIND_SYNC, &sync_paths);

    db::run_transform(&file_paths, &sources, sql, &output_labels, &notices)
        .map_err(|e| e.to_string())
}

// ── run_re_query ──────────────────────────────────────────────────────────────
// Called by: the Imports tab when the user runs an `re_query` step.
// Runs the step's params SQL over the uploaded files, substitutes the values
// into the query template, executes it against RE (live or mock), and writes the
// rows to a temp file. The returned path is what the frontend hands to whichever
// later transform declared this step's `query_output` in its `query_input`.

#[tauri::command]
pub async fn run_re_query(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    step_label: String,
    zip_path: String,
) -> Result<query_step::QueryStepResult, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<query_step::QueryStepResult, String> {
        let loaded = profile::load_from_dir(Path::new(&zip_path)).map_err(|e| e.to_string())?;
        let step = loaded
            .structure
            .steps
            .iter()
            .find(|s| s.label == step_label && s.step_type == "re_query")
            .ok_or_else(|| format!("No re_query step labelled '{}'", step_label))?;

        let transport = resolve_transport(&app)?;
        let run_dir = std::env::temp_dir().join(format!("queries-{}", loaded.structure.id));

        // A params query may join against code tables, so pull them first.
        let ct_dir = std::env::temp_dir().join(format!("codetables-{}", loaded.structure.id));
        let code_table_paths = code_tables::fetch_all(&loaded, &transport, &ct_dir)
            .map_err(|e| e.to_string())?;

        query_step::run_query(
            &loaded,
            step,
            &file_paths,
            &code_table_paths,
            &transport,
            &run_dir,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── run_code_table_sync ───────────────────────────────────────────────────────
// Called by: the Imports tab when the user runs a `code_table_sync` step.
// Runs the step's SQL over the uploaded files, then pushes one create / update /
// delete per returned row to RE's Code Table API. Writes are live only when
// connected (and RE_NXT_MOCK unset) — otherwise they are stubbed like any other
// mock-mode call.

#[tauri::command]
pub async fn run_code_table_sync(
    app: AppHandle,
    file_paths: HashMap<String, String>,
    step_label: String,
    zip_path: String,
) -> Result<code_tables::SyncResult, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<code_tables::SyncResult, String> {
        let loaded = profile::load_from_dir(Path::new(&zip_path)).map_err(|e| e.to_string())?;
        let step = loaded
            .structure
            .steps
            .iter()
            .find(|s| s.label == step_label && s.step_type == "code_table_sync")
            .ok_or_else(|| format!("No code_table_sync step labelled '{}'", step_label))?;

        let transport = resolve_transport(&app)?;
        let run_dir = std::env::temp_dir().join(format!("codetables-{}", loaded.structure.id));
        code_tables::run_sync(&loaded, step, &file_paths, &transport, &run_dir)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── run_visualization ─────────────────────────────────────────────────────────
// Called by: the Imports tab when the user runs a `visualization` step.
// Runs the step's SELECT over the uploaded files plus whatever upstream results
// it declares, and returns the rows for the frontend viz component to draw.
// Nothing is written and nothing is sent to RE — a visualization only reads what
// earlier steps already produced.

#[tauri::command]
pub async fn run_visualization(
    app: AppHandle,
    file_paths: HashMap<String, String>,   // input_label → file_path
    query_paths: HashMap<String, String>,  // query_output label → result JSON path
    sync_paths: HashMap<String, String>,   // sync_output label → outcome JSON path
    step_label: String,
    zip_path: String,
) -> Result<db::ResultSet, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<db::ResultSet, String> {
        let loaded = profile::load_from_dir(Path::new(&zip_path)).map_err(|e| e.to_string())?;
        let step = loaded
            .structure
            .steps
            .iter()
            .find(|s| s.label == step_label && s.step_type == "visualization")
            .ok_or_else(|| format!("No visualization step labelled '{}'", step_label))?;

        // Either the step names a SELECT, or it shows one declared upstream
        // result verbatim — the zero-SQL case for "just show me what came back".
        let sql = match step.sql.as_deref() {
            Some(name) => loaded
                .sql_files
                .get(name)
                .ok_or_else(|| format!("SQL file '{}' not found in profile", name))?
                .clone(),
            None => default_visualization_sql(step)?,
        };

        // A visualization SELECT may join against code tables like any other.
        let code_table_paths = if loaded.structure.code_tables.is_empty() {
            HashMap::new()
        } else {
            let transport = resolve_transport(&app)?;
            let run_dir = std::env::temp_dir().join(format!("codetables-{}", loaded.structure.id));
            code_tables::fetch_all(&loaded, &transport, &run_dir).map_err(|e| e.to_string())?
        };

        let sources = db::SqlSources::new()
            .with(db::KIND_CODETABLE, &code_table_paths)
            .with(db::KIND_QUERY, &query_paths)
            .with(db::KIND_SYNC, &sync_paths);

        db::select_rows(&file_paths, &sources, &sql).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// The SELECT a visualization gets when it names no `sql` file: read the single
// upstream result it declares straight through. More than one declared source is
// ambiguous, so that case asks the author for SQL saying how to combine them.
fn default_visualization_sql(step: &profile::Step) -> Result<String, String> {
    let mut sources: Vec<(&str, &str)> = Vec::new();
    for label in step.query_input.iter().flatten() {
        sources.push((db::KIND_QUERY, label.as_str()));
    }
    for label in step.sync_input.iter().flatten() {
        sources.push((db::KIND_SYNC, label.as_str()));
    }
    match sources.as_slice() {
        [(kind, label)] => Ok(format!(
            "SELECT * FROM read_json_auto('{{{{{}:{}}}}}')",
            kind, label
        )),
        [] => Err(format!(
            "visualization step '{}' needs either a `sql` file or exactly one \
             query_input / sync_input to display",
            step.label
        )),
        _ => Err(format!(
            "visualization step '{}' declares {} upstream results — name a `sql` \
             file saying how to combine them",
            step.label,
            sources.len()
        )),
    }
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

// Pick the RE transport for a report run. Live when connected to RE NXT, unless
// RE_NXT_MOCK is set (forces the fixture path for offline dev). Blocking — call
// from a blocking thread.
fn resolve_transport(app: &AppHandle) -> Result<Transport, String> {
    let force_mock = std::env::var("RE_NXT_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !force_mock && sky_auth::has_connection(app) {
        let (access_token, subscription_key) =
            sky_auth::live_credentials(app).map_err(|e| e.to_string())?;
        Ok(Transport::Live { access_token, subscription_key })
    } else {
        Ok(Transport::Mock)
    }
}

// ── run_report ────────────────────────────────────────────────────────────────
// Called by: the Reports tab on Refresh.
// Runs the report pipeline — RE queries (live SKY API when connected, else mock
// fixtures) → DuckDB transforms → in-memory result sets keyed by transform
// output (what visualizations bind to). `zip_path` is the extracted temp dir,
// like run_profile. async + spawn_blocking because the live path does network
// I/O (reqwest::blocking would panic on the async runtime).

#[tauri::command]
pub async fn run_report(
    app: AppHandle,
    zip_path: String,
    param_values: HashMap<String, serde_json::Value>,
) -> Result<ReportRunResult, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ReportRunResult, String> {
        let loaded = profile::load_from_dir(Path::new(&zip_path)).map_err(|e| e.to_string())?;
        let transport = resolve_transport(&app)?;
        report::run_report(&loaded, &param_values, &transport).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── run_report_action ───────────────────────────────────────────────────────────
// Called by: the Reports tab when the user clicks an action button (e.g. "Create
// Query in RE"). Re-runs the pipeline and performs the write-back (live POST or
// mock stub).

#[tauri::command]
pub async fn run_report_action(
    app: AppHandle,
    zip_path: String,
    action_id: String,
    param_values: HashMap<String, serde_json::Value>,
) -> Result<ActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ActionResult, String> {
        let loaded = profile::load_from_dir(Path::new(&zip_path)).map_err(|e| e.to_string())?;
        let transport = resolve_transport(&app)?;
        report::run_report_action(&loaded, &action_id, &param_values, &transport)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

// ── save_profile ──────────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx Save button.
// Repacks the supplied files into the user profile's .import zip and returns
// the refreshed summary + loaded contents.

#[tauri::command]
pub fn save_profile(
    zip_path: String,
    files: Vec<ProfileFileEntry>,
) -> Result<ProfileMutation, String> {
    let (summary, loaded) = profile::save_user_profile(&zip_path, &files)
        .map_err(|e| e.to_string())?;
    Ok(ProfileMutation { summary, loaded })
}

// ── new_profile ───────────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx 'New profile' button.

#[tauri::command]
pub fn new_profile(app: AppHandle) -> Result<ProfileMutation, String> {
    let dir = user_profiles_dir(&app)?;
    let (summary, loaded) = profile::create_new_profile(&dir)
        .map_err(|e| e.to_string())?;
    Ok(ProfileMutation { summary, loaded })
}

// ── duplicate_profile ─────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx when the user opens a built-in (auto-duplicate)
// or clicks 'Duplicate' on a user profile.

#[tauri::command]
pub fn duplicate_profile(
    app: AppHandle,
    source_zip_path: String,
) -> Result<ProfileMutation, String> {
    let dir = user_profiles_dir(&app)?;
    let (summary, loaded) = profile::duplicate_profile(&source_zip_path, &dir)
        .map_err(|e| e.to_string())?;
    Ok(ProfileMutation { summary, loaded })
}

// ── delete_profile ────────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx 'Delete' button. Refuses built-ins.

#[tauri::command]
pub fn delete_profile(zip_path: String) -> Result<(), String> {
    profile::delete_user_profile(&zip_path).map_err(|e| e.to_string())
}

// ── validate_profile ──────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx 'Validate' button. Pure function over the
// in-memory file set — does not touch disk.

#[tauri::command]
pub fn validate_profile(files: Vec<ProfileFileEntry>) -> Result<ValidationReport, String> {
    Ok(validate::validate_profile(&files))
}

// ── scaffold_missing ──────────────────────────────────────────────────────────
// Called by: SettingsPanel.tsx 'Scaffold missing files' action. Returns the
// updated file list; the frontend swaps it into the editor and the user
// presses Save to persist.

#[tauri::command]
pub fn scaffold_missing(files: Vec<ProfileFileEntry>) -> Result<Vec<ProfileFileEntry>, String> {
    validate::scaffold_missing(&files).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::re_calls::Transport;

    fn demo() -> LoadedProfile {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../profiles/src/re_query_demo");
        profile::load_from_dir(&dir).expect("re_query_demo should load")
    }

    fn vendor_input(loaded: &LoadedProfile) -> HashMap<String, String> {
        let csv = loaded.temp_dir.join("test-files").join("sample_vendor.csv");
        HashMap::from([("Vendor".to_string(), csv.to_string_lossy().to_string())])
    }

    fn step_by_type<'a>(loaded: &'a LoadedProfile, step_type: &str) -> &'a profile::Step {
        loaded
            .structure
            .steps
            .iter()
            .find(|s| s.step_type == step_type)
            .unwrap_or_else(|| panic!("demo has a {} step", step_type))
    }

    // The visualization step's SELECT is run the same way run_visualization runs
    // it: query result on disk, {{query:Label}} resolved through SqlSources.
    #[test]
    fn visualization_sql_renders_the_query_result() {
        let loaded = demo();
        let res = crate::query_step::run_query(
            &loaded,
            step_by_type(&loaded, "re_query"),
            &vendor_input(&loaded),
            &HashMap::new(),
            &Transport::Mock,
            &std::env::temp_dir().join("viz-step-test"),
        )
        .expect("query runs");

        let step = step_by_type(&loaded, "visualization");
        let sql = loaded
            .sql_files
            .get(step.sql.as_deref().expect("demo viz names a sql file"))
            .expect("sql present");
        let query_paths = HashMap::from([(res.query_output.clone(), res.path.clone())]);
        let sources = db::SqlSources::new().with(db::KIND_QUERY, &query_paths);
        let rows = db::select_rows(&vendor_input(&loaded), &sources, sql).expect("viz runs");

        // One row per record RE returned, labelled with what the import will do.
        // The fixture's four records line up with the sample file as: two emails
        // changed, one already matches, one record the vendor file never sent.
        assert_eq!(rows.rows.len(), 4, "columns: {:?}", rows.columns);
        let outcome = rows.columns.iter().position(|c| c == "Outcome").unwrap();
        let outcomes: Vec<&str> = rows.rows.iter().map(|r| r[outcome].as_str()).collect();
        assert_eq!(outcomes.iter().filter(|o| **o == "will update").count(), 2);
        assert_eq!(outcomes.iter().filter(|o| **o == "unchanged").count(), 1);
        assert_eq!(outcomes.iter().filter(|o| **o == "not in file").count(), 1);
    }

    // With no `sql`, a single declared upstream result is shown verbatim.
    #[test]
    fn default_sql_reads_the_one_declared_source() {
        let mut step = demo().structure.steps[0].clone();
        step.sql = None;
        step.query_input = Some(vec!["RERecords".to_string()]);
        step.sync_input = None;
        assert_eq!(
            default_visualization_sql(&step).unwrap(),
            "SELECT * FROM read_json_auto('{{query:RERecords}}')"
        );
    }

    // Zero or several sources are ambiguous — the author has to write the SELECT.
    #[test]
    fn default_sql_needs_exactly_one_source() {
        let mut step = demo().structure.steps[0].clone();
        step.sql = None;
        step.query_input = None;
        step.sync_input = None;
        assert!(default_visualization_sql(&step).is_err());

        step.query_input = Some(vec!["A".to_string(), "B".to_string()]);
        assert!(default_visualization_sql(&step).is_err());
    }
}
