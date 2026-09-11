// api.rs
//
// The application's operations as plain functions — the one surface both
// shells call. The Tauri shell wraps each in a #[tauri::command]; the web
// shell wraps each in an HTTP handler. Everything here works on a Ctx
// (where profiles and session workspaces live, and how to reach RE) plus
// wire-shaped arguments; nothing here knows about IPC, HTTP, or windows.
//
// Path discipline: the client never sees a server path. Profiles are
// addressed as "builtin://<file>" / "user://<file>", a loaded profile is
// addressed by its `session_id`, and files produced by steps are addressed
// by artifact ids relative to that session (workspace.rs enforces
// containment). The only raw paths that cross this boundary are the local
// input files the desktop shell picks via its native dialog — the web shell
// replaces those with uploads into the session's inputs dir.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::code_tables;
use crate::db::{self, NoticeInput, TransformResult, ValidationResult};
use crate::errors::AppError;
use crate::profile::{self, LoadedProfile, NoticeQuery, ProfileFileEntry, ProfileSummary};
use crate::query_step;
use crate::re_calls::Transport;
use crate::report::{self, ActionResult, ReportRunResult};
use crate::user_input;
use crate::validate::{self, ValidationReport};
use crate::workspace::{self, Workspace};

// Sessions untouched for this long are deleted by the best-effort reaper that
// runs on each profile load.
const SESSION_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

// Everything an operation needs from its host: where state lives on disk and
// how to reach RE. The desktop shell builds one per call from its AppHandle;
// the web shell builds one per request from its config + session.
pub struct Ctx {
    // Parent of all session dirs (one subdir per session_id).
    pub workspaces_root: PathBuf,
    // Where user .import bundles live ("user://<file>" resolves here).
    pub user_profiles_dir: PathBuf,
    // Resolves lazily so operations that never touch RE never pay for it —
    // and so the shell owns credential storage (sky_auth on desktop, the
    // server's credential store on the web).
    pub transport: Box<dyn Fn() -> Result<Transport, AppError> + Send + Sync>,
}

impl Ctx {
    fn transport(&self) -> Result<Transport, AppError> {
        (self.transport)()
    }
}

// Combined return for create / duplicate / save — the frontend wants both
// the new sidebar summary and the freshly extracted contents in one round-trip.
#[derive(Serialize)]
pub struct ProfileMutation {
    pub summary: ProfileSummary,
    pub loaded: LoadedProfile,
}

// ── profile refs ──────────────────────────────────────────────────────────────
// "builtin://<file>" (embedded) or "user://<file>" (in user_profiles_dir).
// The filename in a user ref must be a bare *.import name — no separators, no
// traversal — so a client-supplied ref can only ever land inside that dir.

const BUILTIN_PREFIX: &str = "builtin://";
const USER_PREFIX: &str = "user://";

enum ProfileRef<'a> {
    Builtin(&'a str),
    User(PathBuf),
}

fn resolve_profile_ref<'a>(ctx: &Ctx, r: &'a str) -> Result<ProfileRef<'a>, AppError> {
    if let Some(name) = r.strip_prefix(BUILTIN_PREFIX) {
        return Ok(ProfileRef::Builtin(name));
    }
    if let Some(name) = r.strip_prefix(USER_PREFIX) {
        let ok = !name.is_empty()
            && !name.contains('/')
            && !name.contains('\\')
            && !name.contains("..")
            && Path::new(name).extension().and_then(|e| e.to_str()) == Some("import");
        if !ok {
            return Err(AppError::ParseError(format!("Invalid profile ref: {:?}", r)));
        }
        return Ok(ProfileRef::User(ctx.user_profiles_dir.join(name)));
    }
    Err(AppError::ParseError(format!(
        "Invalid profile ref: {:?} (expected builtin:// or user://)",
        r
    )))
}

fn user_ref_for(path: &Path) -> String {
    format!(
        "{}{}",
        USER_PREFIX,
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

// Rewrite the absolute paths list_user_profiles reports into user:// refs so
// the wire carries no server paths.
fn to_user_refs(mut summaries: Vec<ProfileSummary>) -> Vec<ProfileSummary> {
    for s in &mut summaries {
        s.zip_path = user_ref_for(Path::new(&s.zip_path));
    }
    summaries
}

// Stamp the session identity onto a freshly loaded profile before it crosses
// the wire.
fn stamp(mut loaded: LoadedProfile, ws: &Workspace) -> LoadedProfile {
    loaded.session_id = ws.session_id().to_string();
    loaded.asset_base = ws.profile_dir().to_string_lossy().replace('\\', "/");
    loaded
}

fn open_session<'a>(ctx: &Ctx, session_id: &str) -> Result<(Workspace, LoadedProfile), AppError> {
    let ws = Workspace::open(&ctx.workspaces_root, session_id)?;
    let loaded = profile::load_from_dir(&ws.profile_dir())?;
    Ok((ws, loaded))
}

// Resolve a map of client-supplied artifact ids into absolute paths for the
// engine. Used for the query/sync results a transform declares.
fn resolve_ids(
    ws: &Workspace,
    ids: &HashMap<String, String>,
) -> Result<HashMap<String, String>, AppError> {
    ids.iter()
        .map(|(label, id)| {
            ws.resolve(id)
                .map(|p| (label.clone(), p.to_string_lossy().to_string()))
        })
        .collect()
}

// ── list_profiles ─────────────────────────────────────────────────────────────

pub fn list_profiles(ctx: &Ctx) -> Result<Vec<ProfileSummary>, AppError> {
    // Best-effort: ensure the dir exists so the user can drop files there
    // without having to mkdir it themselves. Don't fail listing if it can't.
    let _ = std::fs::create_dir_all(&ctx.user_profiles_dir);

    let mut out = profile::list_builtin_profiles()?;
    out.extend(to_user_refs(profile::list_user_profiles(
        &ctx.user_profiles_dir,
    )?));
    Ok(out)
}

// ── load_profile ──────────────────────────────────────────────────────────────
// Mints a fresh session, extracts the bundle into it, and returns the parsed
// profile stamped with the session_id every later call echoes back.

pub fn load_profile(ctx: &Ctx, profile_ref: &str) -> Result<LoadedProfile, AppError> {
    // Piggyback session cleanup on the operation that creates sessions.
    workspace::reap_stale(&ctx.workspaces_root, SESSION_MAX_AGE);

    let ws = Workspace::create(&ctx.workspaces_root)?;
    let loaded = match resolve_profile_ref(ctx, profile_ref)? {
        ProfileRef::Builtin(name) => profile::load_builtin_into(name, &ws.profile_dir())?,
        ProfileRef::User(path) => profile::load_profile_into(&path, &ws.profile_dir())?,
    };
    Ok(stamp(loaded, &ws))
}

// ── validate_file ─────────────────────────────────────────────────────────────
// Checks a file's columns against the profile's validation rules for that
// input. `file_path` is a local file the shell registered (desktop: the
// native dialog's pick; web: an uploaded file inside the session).

pub fn validate_file(
    ctx: &Ctx,
    file_path: &str,
    input_label: &str,
    session_id: &str,
) -> Result<ValidationResult, AppError> {
    let (_ws, loaded) = open_session(ctx, session_id)?;

    let input_def = loaded
        .structure
        .inputs
        .iter()
        .find(|i| i.label == input_label)
        .ok_or_else(|| {
            AppError::ParseError(format!("No input definition found for '{}'", input_label))
        })?;

    let validations = input_def.validation.as_deref().unwrap_or(&[]);
    db::validate_file(Path::new(file_path), validations)
}

// ── run_profile ───────────────────────────────────────────────────────────────
// Executes one sql_transform's SQL against the attached input files, writing
// each declared output into a fresh run dir inside the session.
//
// `file_paths` maps input label → local file path; `query_ids` / `sync_ids` /
// `form_ids` map upstream labels → the artifact ids their producing steps
// returned.

pub fn run_profile(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    form_ids: HashMap<String, String>,
    sql_file: &str,
    session_id: &str,
    output_labels: &[String],
) -> Result<TransformResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;

    let sql = loaded
        .sql_files
        .get(sql_file)
        .ok_or_else(|| AppError::ParseError(format!("SQL file '{}' not found in profile", sql_file)))?;

    // Find the transform that owns this sql_file so we can pick up any
    // notice queries it declares. Matches by sql filename — adequate while
    // each transform within a profile names a unique .sql file.
    let notice_defs = find_notices_for_sql(&loaded, sql_file);
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
        let transport = ctx.transport()?;
        code_tables::fetch_all(&loaded, &transport, &ws.codetables_dir())?
    };

    let query_paths = resolve_ids(&ws, &query_ids)?;
    let sync_paths = resolve_ids(&ws, &sync_ids)?;
    let form_paths = resolve_ids(&ws, &form_ids)?;

    // Everything the SQL body may reference beyond {{input:}} / {{output:}}.
    let sources = db::SqlSources::new()
        .with(db::KIND_CODETABLE, &code_table_paths)
        .with(db::KIND_QUERY, &query_paths)
        .with(db::KIND_SYNC, &sync_paths)
        .with(db::KIND_FORM, &form_paths);

    let run_dir = ws.new_run_dir()?;
    let mut result = db::run_transform(&file_paths, &sources, sql, output_labels, &notices, &run_dir)?;

    // The engine reports absolute paths; the wire carries session-relative ids.
    for out in &mut result.outputs {
        out.artifact_id = ws.artifact_id(Path::new(&out.artifact_id))?;
    }
    Ok(result)
}

// ── run_re_query ──────────────────────────────────────────────────────────────

pub fn run_re_query(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    step_label: &str,
    session_id: &str,
) -> Result<query_step::QueryStepResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let step = loaded
        .structure
        .steps
        .iter()
        .find(|s| s.label == step_label && s.step_type == "re_query")
        .ok_or_else(|| AppError::ParseError(format!("No re_query step labelled '{}'", step_label)))?;

    let transport = ctx.transport()?;

    // A params query may join against code tables, so pull them first.
    let code_table_paths = code_tables::fetch_all(&loaded, &transport, &ws.codetables_dir())?;

    let mut result = query_step::run_query(
        &loaded,
        step,
        &file_paths,
        &code_table_paths,
        &transport,
        &ws.queries_dir(),
    )?;
    result.artifact_id = ws.artifact_id(Path::new(&result.artifact_id))?;
    Ok(result)
}

// ── run_code_table_sync ───────────────────────────────────────────────────────

pub fn run_code_table_sync(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    step_label: &str,
    session_id: &str,
) -> Result<code_tables::SyncResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let step = loaded
        .structure
        .steps
        .iter()
        .find(|s| s.label == step_label && s.step_type == "code_table_sync")
        .ok_or_else(|| {
            AppError::ParseError(format!("No code_table_sync step labelled '{}'", step_label))
        })?;

    let transport = ctx.transport()?;
    let mut result =
        code_tables::run_sync(&loaded, step, &file_paths, &transport, &ws.codetables_dir())?;
    if let Some(p) = result.artifact_id.take() {
        result.artifact_id = Some(ws.artifact_id(Path::new(&p))?);
    }
    Ok(result)
}

// ── run_visualization ─────────────────────────────────────────────────────────
// Runs the step's SELECT over the uploaded files plus whatever upstream
// results it declares, and returns the rows for the viz component to draw.
// Nothing is written and nothing is sent to RE.

pub fn run_visualization(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    form_ids: HashMap<String, String>,
    step_label: &str,
    session_id: &str,
) -> Result<db::ResultSet, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let step = loaded
        .structure
        .steps
        .iter()
        .find(|s| s.label == step_label && s.step_type == "visualization")
        .ok_or_else(|| {
            AppError::ParseError(format!("No visualization step labelled '{}'", step_label))
        })?;

    // Either the step names a SELECT, or it shows one declared upstream
    // result verbatim — the zero-SQL case for "just show me what came back".
    let sql = match step.sql.as_deref() {
        Some(name) => loaded
            .sql_files
            .get(name)
            .ok_or_else(|| AppError::ParseError(format!("SQL file '{}' not found in profile", name)))?
            .clone(),
        None => default_visualization_sql(step)?,
    };

    // A visualization SELECT may join against code tables like any other.
    let code_table_paths = if loaded.structure.code_tables.is_empty() {
        HashMap::new()
    } else {
        let transport = ctx.transport()?;
        code_tables::fetch_all(&loaded, &transport, &ws.codetables_dir())?
    };

    let query_paths = resolve_ids(&ws, &query_ids)?;
    let sync_paths = resolve_ids(&ws, &sync_ids)?;
    let form_paths = resolve_ids(&ws, &form_ids)?;

    let sources = db::SqlSources::new()
        .with(db::KIND_CODETABLE, &code_table_paths)
        .with(db::KIND_QUERY, &query_paths)
        .with(db::KIND_SYNC, &sync_paths)
        .with(db::KIND_FORM, &form_paths);

    db::select_rows(&file_paths, &sources, &sql)
}

// ── run_user_input ────────────────────────────────────────────────────────────
// Works out which rows the step needs values for, merges in what the user has
// typed, and publishes the result as {{form:Label}}. Called again on every edit
// — the row set is recomputed from the files each time, so it can't go stale.

pub fn run_user_input(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    form_ids: HashMap<String, String>,
    step_label: &str,
    session_id: &str,
    values: &HashMap<String, HashMap<String, String>>,
) -> Result<user_input::UserInputResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let step = loaded
        .structure
        .steps
        .iter()
        .find(|s| s.label == step_label && s.step_type == "user_input")
        .ok_or_else(|| {
            AppError::ParseError(format!("No user_input step labelled '{}'", step_label))
        })?;

    // rows_sql and options_sql read the same sources a transform does: the
    // uploaded files, the profile's code tables, and any upstream result the
    // step declares — including an earlier form, so one form can ask about the
    // rows another one resolved. Ordering is what keeps that acyclic, and the
    // upstream-family check enforces it exactly as it does for queries.
    let code_table_paths = if loaded.structure.code_tables.is_empty() {
        HashMap::new()
    } else {
        let transport = ctx.transport()?;
        code_tables::fetch_all(&loaded, &transport, &ws.codetables_dir())?
    };
    let query_paths = resolve_ids(&ws, &query_ids)?;
    let sync_paths = resolve_ids(&ws, &sync_ids)?;
    let form_paths = resolve_ids(&ws, &form_ids)?;

    let sources = db::SqlSources::new()
        .with(db::KIND_CODETABLE, &code_table_paths)
        .with(db::KIND_QUERY, &query_paths)
        .with(db::KIND_SYNC, &sync_paths)
        .with(db::KIND_FORM, &form_paths);

    let mut result = user_input::run_user_input(
        &loaded,
        step,
        &file_paths,
        &sources,
        values,
        &ws.forms_dir(),
    )?;
    result.artifact_id = ws.artifact_id(Path::new(&result.artifact_id))?;
    Ok(result)
}

// ── reports ───────────────────────────────────────────────────────────────────

pub fn run_report(
    ctx: &Ctx,
    session_id: &str,
    param_values: &HashMap<String, serde_json::Value>,
) -> Result<ReportRunResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let transport = ctx.transport()?;
    let run_dir = ws.new_run_dir()?;
    report::run_report(&loaded, param_values, &transport, &run_dir)
}

pub fn run_report_action(
    ctx: &Ctx,
    session_id: &str,
    action_id: &str,
    param_values: &HashMap<String, serde_json::Value>,
) -> Result<ActionResult, AppError> {
    let (ws, loaded) = open_session(ctx, session_id)?;
    let transport = ctx.transport()?;
    let run_dir = ws.new_run_dir()?;
    report::run_report_action(&loaded, action_id, param_values, &transport, &run_dir)
}

// ── artifacts ─────────────────────────────────────────────────────────────────
// Resolve an artifact id back to the file inside its session. The desktop
// shell copies it to the Save-As destination; the web shell streams it.

pub fn resolve_artifact(ctx: &Ctx, session_id: &str, artifact_id: &str) -> Result<PathBuf, AppError> {
    let ws = Workspace::open(&ctx.workspaces_root, session_id)?;
    let path = ws.resolve(artifact_id)?;
    if !path.is_file() {
        return Err(AppError::IoError(format!(
            "Artifact '{}' not found — re-run the step that produced it",
            artifact_id
        )));
    }
    Ok(path)
}

// ── profile editor ────────────────────────────────────────────────────────────
// Each mutation re-extracts the resulting bundle into a fresh session so the
// editor's in-memory LoadedProfile keeps matching what's on disk.

pub fn save_profile(
    ctx: &Ctx,
    profile_ref: &str,
    files: &[ProfileFileEntry],
) -> Result<ProfileMutation, AppError> {
    let path = match resolve_profile_ref(ctx, profile_ref)? {
        ProfileRef::User(p) => p,
        ProfileRef::Builtin(_) => {
            return Err(AppError::ParseError(
                "Built-in profiles are read-only. Duplicate first to edit.".into(),
            ))
        }
    };
    let ws = Workspace::create(&ctx.workspaces_root)?;
    let (mut summary, loaded) =
        profile::save_user_profile(&path.to_string_lossy(), files, &ws.profile_dir())?;
    summary.zip_path = user_ref_for(&path);
    Ok(ProfileMutation {
        summary,
        loaded: stamp(loaded, &ws),
    })
}

pub fn new_profile(ctx: &Ctx) -> Result<ProfileMutation, AppError> {
    let ws = Workspace::create(&ctx.workspaces_root)?;
    let (mut summary, loaded) =
        profile::create_new_profile(&ctx.user_profiles_dir, &ws.profile_dir())?;
    summary.zip_path = user_ref_for(Path::new(&summary.zip_path));
    Ok(ProfileMutation {
        summary,
        loaded: stamp(loaded, &ws),
    })
}

pub fn duplicate_profile(ctx: &Ctx, source_ref: &str) -> Result<ProfileMutation, AppError> {
    // A builtin:// source is legitimate here — that's how "edit a built-in"
    // works — so resolve to the original string form profile.rs understands.
    let source: String = match resolve_profile_ref(ctx, source_ref)? {
        ProfileRef::Builtin(_) => source_ref.to_string(),
        ProfileRef::User(p) => p.to_string_lossy().to_string(),
    };
    let ws = Workspace::create(&ctx.workspaces_root)?;
    let (mut summary, loaded) =
        profile::duplicate_profile(&source, &ctx.user_profiles_dir, &ws.profile_dir())?;
    summary.zip_path = user_ref_for(Path::new(&summary.zip_path));
    Ok(ProfileMutation {
        summary,
        loaded: stamp(loaded, &ws),
    })
}

pub fn delete_profile(ctx: &Ctx, profile_ref: &str) -> Result<(), AppError> {
    let path = match resolve_profile_ref(ctx, profile_ref)? {
        ProfileRef::User(p) => p,
        ProfileRef::Builtin(_) => {
            return Err(AppError::ParseError(
                "Built-in profiles are read-only and cannot be deleted.".into(),
            ))
        }
    };
    profile::delete_user_profile(&path.to_string_lossy())
}

// Pure functions over the in-memory file set — passthroughs so both shells
// find every operation on this one surface.

pub fn validate_profile(files: &[ProfileFileEntry]) -> ValidationReport {
    validate::validate_profile(files)
}

pub fn scaffold_missing(files: &[ProfileFileEntry]) -> Result<Vec<ProfileFileEntry>, AppError> {
    validate::scaffold_missing(files)
}

// ── helpers ───────────────────────────────────────────────────────────────────

// The SELECT a visualization gets when it names no `sql` file: read the single
// upstream result it declares straight through. More than one declared source
// is ambiguous, so that case asks the author for SQL saying how to combine them.
fn default_visualization_sql(step: &profile::Step) -> Result<String, AppError> {
    let mut sources: Vec<(&str, &str)> = Vec::new();
    for label in step.query_input.iter().flatten() {
        sources.push((db::KIND_QUERY, label.as_str()));
    }
    for label in step.sync_input.iter().flatten() {
        sources.push((db::KIND_SYNC, label.as_str()));
    }
    for label in step.form_input.iter().flatten() {
        sources.push((db::KIND_FORM, label.as_str()));
    }
    match sources.as_slice() {
        [(kind, label)] => Ok(format!(
            "SELECT * FROM read_json_auto('{{{{{}:{}}}}}')",
            kind, label
        )),
        [] => Err(AppError::ParseError(format!(
            "visualization step '{}' needs either a `sql` file or exactly one \
             query_input / sync_input / form_input to display",
            step.label
        ))),
        _ => Err(AppError::ParseError(format!(
            "visualization step '{}' declares {} upstream results — name a `sql` \
             file saying how to combine them",
            step.label,
            sources.len()
        ))),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn demo() -> LoadedProfile {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/src/re_query_demo");
        profile::load_from_dir(&dir).expect("re_query_demo should load")
    }

    fn step_by_type<'a>(loaded: &'a LoadedProfile, step_type: &str) -> &'a profile::Step {
        loaded
            .structure
            .steps
            .iter()
            .find(|s| s.step_type == step_type)
            .unwrap_or_else(|| panic!("demo has a {} step", step_type))
    }

    fn test_ctx(name: &str) -> Ctx {
        let base = std::env::temp_dir()
            .join(format!("api-test-{}", name))
            .join(workspace::unique_token());
        Ctx {
            workspaces_root: base.join("sessions"),
            user_profiles_dir: base.join("profiles"),
            transport: Box::new(|| Ok(Transport::Mock)),
        }
    }

    // With no `sql`, a single declared upstream result is shown verbatim.
    #[test]
    fn default_sql_reads_the_one_declared_source() {
        let mut step = demo().structure.steps[0].clone();
        step.sql = None;
        step.query_input = Some(vec!["RERecords".to_string()]);
        step.sync_input = None;
        step.form_input = None;
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
        step.form_input = None;
        assert!(default_visualization_sql(&step).is_err());

        step.query_input = Some(vec!["A".to_string(), "B".to_string()]);
        assert!(default_visualization_sql(&step).is_err());
    }

    #[test]
    fn profile_refs_reject_traversal() {
        let ctx = test_ctx("refs");
        assert!(resolve_profile_ref(&ctx, "user://../../etc/passwd").is_err());
        assert!(resolve_profile_ref(&ctx, "user://a/b.import").is_err());
        assert!(resolve_profile_ref(&ctx, "user://notes.txt").is_err());
        assert!(resolve_profile_ref(&ctx, "/tmp/x.import").is_err());
        assert!(resolve_profile_ref(&ctx, "user://ok.import").is_ok());
        assert!(resolve_profile_ref(&ctx, "builtin://test1.import").is_ok());
    }

    // The full desktop flow against a built-in: load mints a session, a
    // re_query step writes an artifact into it, and the id round-trips
    // through a downstream visualization — no absolute path on the wire.
    #[test]
    fn end_to_end_session_flow() {
        let ctx = test_ctx("e2e");
        let loaded = load_profile(&ctx, "builtin://re_query_demo.import").expect("load ok");
        assert!(!loaded.session_id.is_empty());

        // The sample file lives in the profile's source tree — build.sh
        // excludes test-files/ from the packed bundle, so it isn't in the
        // extracted session.
        let csv = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../profiles/src/re_query_demo/test-files/sample_vendor.csv");
        let file_paths =
            HashMap::from([("Vendor".to_string(), csv.to_string_lossy().to_string())]);

        let q_step = step_by_type(&loaded, "re_query").label.clone();
        let res = run_re_query(&ctx, file_paths.clone(), &q_step, &loaded.session_id)
            .expect("query ok");
        assert!(
            !res.artifact_id.contains(':') && !res.artifact_id.starts_with('/'),
            "artifact id leaked an absolute path: {}",
            res.artifact_id
        );

        let viz_step = step_by_type(&loaded, "visualization").label.clone();
        let query_ids = HashMap::from([(res.query_output.clone(), res.artifact_id.clone())]);
        let rows = run_visualization(
            &ctx,
            file_paths,
            query_ids,
            HashMap::new(),
            HashMap::new(),
            &viz_step,
            &loaded.session_id,
        )
        .expect("viz ok");
        assert_eq!(rows.rows.len(), 4, "columns: {:?}", rows.columns);
    }

    // The user_input round trip, through the real API surface: a form derives
    // its rows from the uploaded file, the values the user typed are published,
    // and a downstream transform reads them as {{form:Label}} using nothing but
    // the artifact id that crossed the wire.
    #[test]
    fn user_input_values_reach_a_downstream_transform() {
        let ctx = test_ctx("form-e2e");
        std::fs::create_dir_all(&ctx.user_profiles_dir).unwrap();

        let structure = "\
id: form_demo
name: Form Demo
version: 1.0.0
min_app_version: 0.1.0
inputs:
  - label: Rows
    type: csv
    required: true
outputs:
  - label: Out
    type: csv
steps:
  - label: Upload
    type: file_input
    input: [Rows]
  - label: Ask
    type: user_input
    input: [Rows]
    rows_sql: terms.sql
    key_column: term
    fields:
      - { id: date_from, label: Start, type: date, required: true }
    form_output: Terms
  - label: Build
    type: sql_transform
    input: [Rows]
    form_input: [Terms]
    sql: build.sql
    output: [Out]
";
        let files = vec![
            ProfileFileEntry { path: "structure.yaml".into(), content: structure.into() },
            ProfileFileEntry {
                path: "instructions.md".into(),
                content: "# Form Demo\n".into(),
            },
            ProfileFileEntry {
                path: "sql/terms.sql".into(),
                content: "SELECT DISTINCT term FROM read_csv_auto('{{input:Rows}}') ORDER BY term"
                    .into(),
            },
            // Inner join on the collected dates: a term left blank drops out.
            ProfileFileEntry {
                path: "sql/build.sql".into(),
                content: "SELECT r.id, t.date_from \
                          FROM read_csv_auto('{{input:Rows}}') r \
                          JOIN read_json('{{form:Terms}}', columns={key: 'VARCHAR', date_from: 'VARCHAR'}) t \
                          ON r.term = t.key \
                          WHERE t.date_from IS NOT NULL"
                    .into(),
            },
        ];
        let zip = ctx.user_profiles_dir.join("form_demo.import");
        let extract = std::env::temp_dir().join(workspace::unique_token());
        profile::save_user_profile(&zip.to_string_lossy(), &files, &extract).expect("bundle saved");

        let loaded = load_profile(&ctx, "user://form_demo.import").expect("load ok");

        let csv = extract.join("rows.csv");
        std::fs::write(&csv, "id,term\n1,F25\n2,SP26\n3,F25\n").unwrap();
        let file_paths =
            HashMap::from([("Rows".to_string(), csv.to_string_lossy().to_string())]);

        // First pass: no values yet. Both terms are discovered, and the step is
        // incomplete because the required date is blank on each.
        let discovered = run_user_input(
            &ctx,
            file_paths.clone(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            "Ask",
            &loaded.session_id,
            &HashMap::new(),
        )
        .expect("form runs");
        assert_eq!(discovered.row_count, 2, "rows: {:?}", discovered.rows);
        assert!(!discovered.complete);
        assert!(
            !discovered.artifact_id.starts_with('/') && !discovered.artifact_id.contains(':'),
            "artifact id leaked a path: {}",
            discovered.artifact_id
        );

        // Second pass: one term dated, one left blank.
        let values = HashMap::from([(
            "F25".to_string(),
            HashMap::from([("date_from".to_string(), "2025-09-22".to_string())]),
        )]);
        let filled = run_user_input(
            &ctx,
            file_paths.clone(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            "Ask",
            &loaded.session_id,
            &values,
        )
        .expect("form runs");
        assert!(!filled.complete, "SP26 is still blank");

        // The transform sees the dated term only — two F25 rows, no SP26.
        let result = run_profile(
            &ctx,
            file_paths,
            HashMap::new(),
            HashMap::new(),
            HashMap::from([(filled.form_output.clone(), filled.artifact_id.clone())]),
            "build.sql",
            &loaded.session_id,
            &["Out".to_string()],
        )
        .expect("transform ok");
        assert_eq!(result.outputs[0].row_count, 2);
    }

    // A select whose choices come from options_sql, feeding a second form that
    // derives its rows from the first one's answers. Both are the new pieces:
    // runtime options, and a form reading {{form:…}} rather than only files.
    #[test]
    fn a_form_can_offer_runtime_options_and_feed_another_form() {
        let ctx = test_ctx("form-chain");
        std::fs::create_dir_all(&ctx.user_profiles_dir).unwrap();

        let structure = "\
id: chain_demo
name: Chain Demo
version: 1.0.0
min_app_version: 0.1.0
inputs:
  - label: Rows
    type: csv
    required: true
outputs:
  - label: Out
    type: csv
steps:
  - label: Upload
    type: file_input
    input: [Rows]
  - label: Map
    type: user_input
    input: [Rows]
    rows_sql: unmapped.sql
    key_column: code
    fields:
      - { id: mapped_to, label: Use this, type: select, options_sql: choices.sql }
    form_output: Mapping
  - label: Ask
    type: user_input
    input: [Rows]
    form_input: [Mapping]
    rows_sql: terms.sql
    key_column: term
    fields:
      - { id: note, label: Note, type: text, required: true }
    form_output: Notes
";
        let files = vec![
            ProfileFileEntry { path: "structure.yaml".into(), content: structure.into() },
            ProfileFileEntry { path: "instructions.md".into(), content: "# Chain\n".into() },
            // Codes in the file that aren't already known.
            ProfileFileEntry {
                path: "sql/unmapped.sql".into(),
                content: "SELECT DISTINCT code FROM read_csv_auto('{{input:Rows}}') \
                          WHERE code NOT IN ('KNOWN') ORDER BY code"
                    .into(),
            },
            // The choices — a fixed set here, standing in for a query result.
            ProfileFileEntry {
                path: "sql/choices.sql".into(),
                content: "SELECT * FROM (VALUES ('KNOWN', 'The known one')) AS t(value, label)"
                    .into(),
            },
            // Terms on rows that resolve: known directly, or via the mapping.
            ProfileFileEntry {
                path: "sql/terms.sql".into(),
                content: "WITH m AS (SELECT key, mapped_to FROM \
                            read_json('{{form:Mapping}}', columns={key:'VARCHAR', mapped_to:'VARCHAR'}) \
                            WHERE mapped_to IS NOT NULL) \
                          SELECT DISTINCT r.term FROM read_csv_auto('{{input:Rows}}') r \
                          LEFT JOIN m ON r.code = m.key \
                          WHERE r.code = 'KNOWN' OR m.mapped_to IS NOT NULL ORDER BY r.term"
                    .into(),
            },
        ];
        let zip = ctx.user_profiles_dir.join("chain_demo.import");
        let extract = std::env::temp_dir().join(workspace::unique_token());
        profile::save_user_profile(&zip.to_string_lossy(), &files, &extract).expect("bundle saved");
        let loaded = load_profile(&ctx, "user://chain_demo.import").expect("load ok");

        let csv = extract.join("rows.csv");
        std::fs::write(&csv, "code,term\nKNOWN,F25\nLEGACY,SP26\nGHOST,SU26\n").unwrap();
        let file_paths = HashMap::from([("Rows".to_string(), csv.to_string_lossy().to_string())]);

        // The mapping form offers what options_sql returned.
        let mapping = run_user_input(
            &ctx,
            file_paths.clone(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            "Map",
            &loaded.session_id,
            &HashMap::new(),
        )
        .expect("mapping form runs");
        assert_eq!(mapping.row_count, 2, "GHOST and LEGACY are unmapped");
        let choices = &mapping.options["mapped_to"];
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].value, "KNOWN");
        assert_eq!(choices[0].label, "The known one");

        // Before any mapping, only the directly-known row's term is in play.
        let before = run_user_input(
            &ctx,
            file_paths.clone(),
            HashMap::new(),
            HashMap::new(),
            HashMap::from([(mapping.form_output.clone(), mapping.artifact_id.clone())]),
            "Ask",
            &loaded.session_id,
            &HashMap::new(),
        )
        .expect("second form runs");
        assert_eq!(
            before.rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
            vec!["F25"]
        );

        // Map LEGACY, and SP26 becomes a question the second form now asks.
        let mapped = run_user_input(
            &ctx,
            file_paths.clone(),
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
            "Map",
            &loaded.session_id,
            &HashMap::from([(
                "LEGACY".to_string(),
                HashMap::from([("mapped_to".to_string(), "KNOWN".to_string())]),
            )]),
        )
        .expect("mapping form runs");
        let after = run_user_input(
            &ctx,
            file_paths,
            HashMap::new(),
            HashMap::new(),
            HashMap::from([(mapped.form_output.clone(), mapped.artifact_id.clone())]),
            "Ask",
            &loaded.session_id,
            &HashMap::new(),
        )
        .expect("second form runs");
        assert_eq!(
            after.rows.iter().map(|r| r.key.as_str()).collect::<Vec<_>>(),
            vec!["F25", "SP26"],
            "mapping a code brings its term into play"
        );
        // GHOST was never mapped, so SU26 is still never asked about.
        assert!(!after.rows.iter().any(|r| r.key == "SU26"));
    }

    // Two sessions loading the same profile must not share extraction dirs —
    // the exact concurrency bug the old /tmp/import-tool-<stem>/ layout had.
    #[test]
    fn concurrent_sessions_are_isolated() {
        let ctx = test_ctx("isolation");
        let a = load_profile(&ctx, "builtin://test1.import").expect("load a");
        let b = load_profile(&ctx, "builtin://test1.import").expect("load b");
        assert_ne!(a.session_id, b.session_id);
        assert_ne!(a.asset_base, b.asset_base);
        // Both extractions exist simultaneously.
        assert!(Path::new(&a.asset_base).join("structure.yaml").is_file());
        assert!(Path::new(&b.asset_base).join("structure.yaml").is_file());
    }
}
