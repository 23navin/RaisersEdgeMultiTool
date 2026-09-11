// main.rs — the web shell.
//
// Axum server exposing every core::api operation as POST /api/<command> with
// the same camelCase JSON bodies the Tauri shell receives, plus the three
// web-shaped endpoints the browser transport (src/lib/api.ts) expects:
//
//   POST /api/sessions/{sid}/inputs            multipart upload → input id
//   GET  /api/sessions/{sid}/artifacts/{*id}   download an output artifact
//   GET  /api/sessions/{sid}/assets/{*rel}     instruction images etc.
//   GET  /api/oauth/callback                   Blackbaud's OAuth redirect
//
// The RE NXT connection is established through the same Settings → General
// form the desktop app uses: connect_re_nxt returns an authorization URL, the
// browser visits Blackbaud, and Blackbaud redirects to /api/oauth/callback.
// No loopback listener is needed — a server already has an addressable URL.
//
// Everything else is served from STATIC_DIR (the Vite dist/) as the SPA.
//
// Config, strictly via env:
//   DATA_DIR    all state — user profiles, session workspaces, token file
//               (default ./data; the one path to mount as a volume)
//   BIND_ADDR   listen address              (default 0.0.0.0:8080)
//   STATIC_DIR  built frontend to serve     (default ./dist)
//   MAX_RUNS    concurrent pipeline runs    (default 4; each run is a full
//               in-memory DuckDB plus, live, a SKY poll loop)
//   PUBLIC_URL  the app's externally reachable base URL, e.g.
//               https://multitool.example.org — used to build the OAuth
//               redirect_uri. Falls back to the request's Host header.
//   RE_CLIENT_ID / RE_CLIENT_SECRET / RE_SUBSCRIPTION_KEY / RE_REFRESH_TOKEN
//               the shared RE NXT service account (absent → mock mode)
//   RE_NXT_MOCK force mock mode even when credentials are configured

mod creds;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path as AxPath, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tower_http::services::{ServeDir, ServeFile};

use multitool_core::api::{self, Ctx};
use multitool_core::creds::ConnectionStatus;
use multitool_core::errors::AppError;
use multitool_core::profile::ProfileFileEntry;
use multitool_core::workspace::{self, Workspace};

struct AppState {
    workspaces_root: PathBuf,
    user_profiles_dir: PathBuf,
    creds: creds::CredStore,
    public_url: Option<String>,
    // Bounds concurrent pipeline runs — each is a full in-memory DuckDB, and
    // a live SKY query can hold a blocking thread for minutes.
    run_permits: Semaphore,
}

impl AppState {
    fn ctx(self: &Arc<Self>) -> Ctx {
        let state = Arc::clone(self);
        Ctx {
            workspaces_root: self.workspaces_root.clone(),
            user_profiles_dir: self.user_profiles_dir.clone(),
            transport: Box::new(move || state.creds.transport()),
        }
    }

    fn open_workspace(&self, session_id: &str) -> Result<Workspace, ApiError> {
        Workspace::open(&self.workspaces_root, session_id).map_err(ApiError)
    }

    // The OAuth redirect_uri, which must byte-match what's registered on the
    // Blackbaud application. PUBLIC_URL is authoritative; without it we derive
    // from the request's Host (fine for local dev, but set PUBLIC_URL behind a
    // proxy or the scheme/host will be wrong).
    fn redirect_uri(&self, headers: &axum::http::HeaderMap) -> String {
        if let Some(base) = &self.public_url {
            return format!("{}/api/oauth/callback", base.trim_end_matches('/'));
        }
        let host = headers
            .get(axum::http::header::HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost:8080");
        let scheme = if host.starts_with("localhost") || host.starts_with("127.0.0.1") {
            "http"
        } else {
            "https"
        };
        format!("{}://{}/api/oauth/callback", scheme, host)
    }
}

// AppError → HTTP. The body is the bare message text — the frontend transport
// throws it, matching what a rejected invoke() carries on desktop.
struct ApiError(AppError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            AppError::ParseError(_) => StatusCode::BAD_REQUEST,
            AppError::AuthError(_) => StatusCode::UNAUTHORIZED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, self.0.to_string()).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        ApiError(e)
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

// Run blocking engine work off the async runtime (the live path uses
// reqwest::blocking, which panics inside a Tokio context).
async fn blocking<T, F>(f: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError(AppError::IoError(format!("Worker panicked: {}", e))))?
        .map_err(ApiError)
}

// The web client's asset base is a URL prefix, not a directory. Rewrites the
// path core::api stamped before the profile crosses the wire.
fn weblify(mut loaded: multitool_core::profile::LoadedProfile) -> multitool_core::profile::LoadedProfile {
    loaded.asset_base = format!("/api/sessions/{}/assets", loaded.session_id);
    loaded
}

fn weblify_mutation(mut m: api::ProfileMutation) -> api::ProfileMutation {
    m.loaded = weblify(m.loaded);
    m
}

// Resolve the client's input-file ids into engine paths. The web client only
// ever holds ids minted by the upload endpoint; Workspace::resolve rejects
// absolute paths and traversal, so a crafted value can't reach outside the
// session.
fn resolve_inputs(
    ws: &Workspace,
    file_ids: &HashMap<String, String>,
) -> Result<HashMap<String, String>, ApiError> {
    file_ids
        .iter()
        .map(|(label, id)| {
            ws.resolve(id)
                .map(|p| (label.clone(), p.to_string_lossy().to_string()))
                .map_err(ApiError)
        })
        .collect()
}

// ── request bodies (camelCase, matching the Tauri arg encoding) ──────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoadProfileReq {
    zip_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidateFileReq {
    file_path: String,
    input_label: String,
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunProfileReq {
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    #[serde(default)]
    form_ids: HashMap<String, String>,
    sql_file: String,
    session_id: String,
    output_labels: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StepReq {
    file_paths: HashMap<String, String>,
    step_label: String,
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VisualizationReq {
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    sync_ids: HashMap<String, String>,
    #[serde(default)]
    form_ids: HashMap<String, String>,
    step_label: String,
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserInputReq {
    file_paths: HashMap<String, String>,
    #[serde(default)]
    query_ids: HashMap<String, String>,
    #[serde(default)]
    sync_ids: HashMap<String, String>,
    step_label: String,
    session_id: String,
    #[serde(default)]
    values: HashMap<String, HashMap<String, String>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunReportReq {
    session_id: String,
    param_values: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportActionReq {
    session_id: String,
    action_id: String,
    param_values: HashMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveProfileReq {
    zip_path: String,
    files: Vec<ProfileFileEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateProfileReq {
    source_zip_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ZipPathReq {
    zip_path: String,
}

#[derive(Deserialize)]
struct FilesReq {
    files: Vec<ProfileFileEntry>,
}

// ── handlers ──────────────────────────────────────────────────────────────────

async fn list_profiles(State(s): State<Arc<AppState>>) -> ApiResult<Vec<multitool_core::profile::ProfileSummary>> {
    let ctx = s.ctx();
    Ok(Json(blocking(move || api::list_profiles(&ctx)).await?))
}

async fn load_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<LoadProfileReq>,
) -> ApiResult<multitool_core::profile::LoadedProfile> {
    let ctx = s.ctx();
    let loaded = blocking(move || api::load_profile(&ctx, &req.zip_path)).await?;
    Ok(Json(weblify(loaded)))
}

async fn validate_file(
    State(s): State<Arc<AppState>>,
    Json(req): Json<ValidateFileReq>,
) -> ApiResult<multitool_core::db::ValidationResult> {
    let ws = s.open_workspace(&req.session_id)?;
    let path = ws.resolve(&req.file_path)?.to_string_lossy().to_string();
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || api::validate_file(&ctx, &path, &req.input_label, &req.session_id))
            .await?,
    ))
}

async fn run_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RunProfileReq>,
) -> ApiResult<multitool_core::db::TransformResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || {
            api::run_profile(
                &ctx,
                file_paths,
                req.query_ids,
                req.sync_ids,
                req.form_ids,
                &req.sql_file,
                &req.session_id,
                &req.output_labels,
            )
        })
        .await?,
    ))
}

async fn run_re_query(
    State(s): State<Arc<AppState>>,
    Json(req): Json<StepReq>,
) -> ApiResult<multitool_core::query_step::QueryStepResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || api::run_re_query(&ctx, file_paths, &req.step_label, &req.session_id))
            .await?,
    ))
}

async fn run_code_table_sync(
    State(s): State<Arc<AppState>>,
    Json(req): Json<StepReq>,
) -> ApiResult<multitool_core::code_tables::SyncResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || {
            api::run_code_table_sync(&ctx, file_paths, &req.step_label, &req.session_id)
        })
        .await?,
    ))
}

async fn run_visualization(
    State(s): State<Arc<AppState>>,
    Json(req): Json<VisualizationReq>,
) -> ApiResult<multitool_core::db::ResultSet> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || {
            api::run_visualization(
                &ctx,
                file_paths,
                req.query_ids,
                req.sync_ids,
                req.form_ids,
                &req.step_label,
                &req.session_id,
            )
        })
        .await?,
    ))
}

async fn run_user_input(
    State(s): State<Arc<AppState>>,
    Json(req): Json<UserInputReq>,
) -> ApiResult<multitool_core::user_input::UserInputResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || {
            api::run_user_input(
                &ctx,
                file_paths,
                req.query_ids,
                req.sync_ids,
                &req.step_label,
                &req.session_id,
                &req.values,
            )
        })
        .await?,
    ))
}

async fn run_report(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RunReportReq>,
) -> ApiResult<multitool_core::report::ReportRunResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || api::run_report(&ctx, &req.session_id, &req.param_values)).await?,
    ))
}

async fn run_report_action(
    State(s): State<Arc<AppState>>,
    Json(req): Json<ReportActionReq>,
) -> ApiResult<multitool_core::report::ActionResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ctx = s.ctx();
    Ok(Json(
        blocking(move || {
            api::run_report_action(&ctx, &req.session_id, &req.action_id, &req.param_values)
        })
        .await?,
    ))
}

// ── profile editor ────────────────────────────────────────────────────────────

async fn save_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<SaveProfileReq>,
) -> ApiResult<api::ProfileMutation> {
    let ctx = s.ctx();
    let m = blocking(move || api::save_profile(&ctx, &req.zip_path, &req.files)).await?;
    Ok(Json(weblify_mutation(m)))
}

async fn new_profile(State(s): State<Arc<AppState>>) -> ApiResult<api::ProfileMutation> {
    let ctx = s.ctx();
    let m = blocking(move || api::new_profile(&ctx)).await?;
    Ok(Json(weblify_mutation(m)))
}

async fn duplicate_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<DuplicateProfileReq>,
) -> ApiResult<api::ProfileMutation> {
    let ctx = s.ctx();
    let m = blocking(move || api::duplicate_profile(&ctx, &req.source_zip_path)).await?;
    Ok(Json(weblify_mutation(m)))
}

async fn delete_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<ZipPathReq>,
) -> ApiResult<()> {
    let ctx = s.ctx();
    Ok(Json(blocking(move || api::delete_profile(&ctx, &req.zip_path)).await?))
}

async fn validate_profile(
    Json(req): Json<FilesReq>,
) -> ApiResult<multitool_core::validate::ValidationReport> {
    Ok(Json(api::validate_profile(&req.files)))
}

async fn scaffold_missing(Json(req): Json<FilesReq>) -> ApiResult<Vec<ProfileFileEntry>> {
    Ok(Json(api::scaffold_missing(&req.files).map_err(ApiError)?))
}

// ── RE NXT connection ─────────────────────────────────────────────────────────
// The same three commands the desktop shell exposes, so the Settings → General
// panel works unchanged. The difference is only how the authorization code is
// obtained: the browser is sent to Blackbaud and comes back to
// /api/oauth/callback below.
//
// SECURITY (deferred): the connection is server-wide and these endpoints are
// unauthenticated, so anyone who can reach the server can connect, replace, or
// disconnect it. Gate them behind the login that comes with real auth.

async fn re_nxt_status(State(s): State<Arc<AppState>>) -> ApiResult<ConnectionStatus> {
    Ok(Json(s.creds.status()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectReq {
    client_id: String,
    client_secret: String,
    subscription_key: String,
}

// The web analogue of the desktop's "open a browser and wait". Returns the
// Blackbaud URL for the client to navigate to; the connection isn't persisted
// until the user finishes signing in and Blackbaud calls us back.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectStarted {
    authorize_url: String,
    redirect_uri: String,
}

async fn connect_re_nxt(
    State(s): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ConnectReq>,
) -> ApiResult<ConnectStarted> {
    let redirect_uri = s.redirect_uri(&headers);
    // The single most common cause of a failed handshake is this value not
    // byte-matching what's registered on the Blackbaud application, so log it.
    tracing::info!(%redirect_uri, "starting RE NXT authorization");
    let authorize_url = s
        .creds
        .begin_connect(
            req.client_id,
            req.client_secret,
            req.subscription_key,
            redirect_uri.clone(),
        )
        .map_err(ApiError)?;
    Ok(Json(ConnectStarted { authorize_url, redirect_uri }))
}

async fn disconnect_re_nxt(State(s): State<Arc<AppState>>) -> ApiResult<()> {
    s.creds.disconnect().map_err(ApiError)?;
    Ok(Json(()))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

// Where Blackbaud sends the user after they sign in. Exchanges the code, then
// bounces back to the SPA — a redirect rather than JSON, because a human is
// looking at this response in their browser.
async fn oauth_callback(
    State(s): State<Arc<AppState>>,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let outcome = match (q.error, q.code, q.state) {
        (Some(err), _, _) => Err(format!(
            "{}{}",
            err,
            q.error_description.map(|d| format!(": {}", d)).unwrap_or_default()
        )),
        (None, Some(code), Some(state)) => {
            // Blocking token exchange — keep it off the async runtime.
            let creds_state = Arc::clone(&s);
            tokio::task::spawn_blocking(move || creds_state.creds.finish_connect(&state, &code))
                .await
                .map_err(|e| format!("Worker panicked: {}", e))
                .and_then(|r| r.map_err(|e| e.to_string()))
                .map(|_| ())
        }
        _ => Err("The redirect carried no authorization code.".to_string()),
    };

    match outcome {
        Ok(()) => {
            tracing::info!("RE NXT connected");
            axum::response::Redirect::to("/?connected=1").into_response()
        }
        Err(msg) => {
            tracing::warn!(error = %msg, "RE NXT connect failed");
            // Send the message back through the SPA so the settings panel can
            // show it, rather than stranding the user on a blank error page.
            axum::response::Redirect::to(&format!(
                "/?connect_error={}",
                multitool_core::creds::urlencode(&msg)
            ))
            .into_response()
        }
    }
}

// ── session file endpoints ────────────────────────────────────────────────────

// Upload one input file into the session. Returns { path: <artifact id> } —
// the value the client then passes in filePaths, resolved server-side.
async fn upload_input(
    State(s): State<Arc<AppState>>,
    AxPath(sid): AxPath<String>,
    mut multipart: Multipart,
) -> ApiResult<serde_json::Value> {
    let ws = s.open_workspace(&sid)?;
    let inputs_dir = ws.inputs_dir();
    tokio::fs::create_dir_all(&inputs_dir)
        .await
        .map_err(|e| ApiError(AppError::IoError(e.to_string())))?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError(AppError::ParseError(e.to_string())))?
    {
        if field.name() != Some("file") {
            continue;
        }
        // Keep only the basename; the token prefix keeps re-uploads distinct.
        let original = field.file_name().unwrap_or("upload").to_string();
        let base = std::path::Path::new(&original)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".into());
        let dest = inputs_dir.join(format!("{}-{}", workspace::unique_token(), base));

        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError(AppError::IoError(format!("Upload failed: {}", e))))?;
        tokio::fs::write(&dest, &bytes)
            .await
            .map_err(|e| ApiError(AppError::IoError(e.to_string())))?;

        let id = ws.artifact_id(&dest)?;
        return Ok(Json(serde_json::json!({ "path": id })));
    }
    Err(ApiError(AppError::ParseError(
        "Multipart body had no 'file' field".into(),
    )))
}

#[derive(Deserialize)]
struct DownloadQuery {
    name: Option<String>,
}

// Stream an artifact back as a download.
async fn download_artifact(
    State(s): State<Arc<AppState>>,
    AxPath((sid, artifact_id)): AxPath<(String, String)>,
    Query(q): Query<DownloadQuery>,
) -> Result<Response, ApiError> {
    let ws = s.open_workspace(&sid)?;
    let path = ws.resolve(&artifact_id)?;
    let bytes = tokio::fs::read(&path).await.map_err(|_| {
        ApiError(AppError::IoError(format!(
            "Artifact '{}' not found — re-run the step that produced it",
            artifact_id
        )))
    })?;
    let name = q.name.unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "download".into())
    });
    // Quote-safe: the name came from a label the profile author wrote.
    let disposition = format!("attachment; filename=\"{}\"", name.replace('"', ""));
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        bytes,
    )
        .into_response())
}

// Serve a file out of the session's extracted profile (instruction images).
async fn session_asset(
    State(s): State<Arc<AppState>>,
    AxPath((sid, rel)): AxPath<(String, String)>,
) -> Result<Response, ApiError> {
    let ws = s.open_workspace(&sid)?;
    // Resolve under profile/ — the same containment rules as any artifact id.
    let path = ws.resolve(&format!("profile/{}", rel))?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ApiError(AppError::IoError(format!("Asset '{}' not found", rel))))?;
    let mime = match path.extension().and_then(|e| e.to_str()) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

// ── main ──────────────────────────────────────────────────────────────────────

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let data_dir = PathBuf::from(env_or("DATA_DIR", "./data"));
    std::fs::create_dir_all(&data_dir).expect("DATA_DIR must be creatable");

    let force_mock = std::env::var("RE_NXT_MOCK")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let creds = creds::CredStore::new(&data_dir, force_mock);
    let max_runs: usize = env_or("MAX_RUNS", "4").parse().expect("MAX_RUNS must be a number");

    let public_url = std::env::var("PUBLIC_URL").ok();
    tracing::info!(
        data_dir = %data_dir.display(),
        mode = if creds.is_connected() { "live" } else { "mock" },
        public_url = public_url.as_deref().unwrap_or("(derived from Host)"),
        max_runs,
        "starting"
    );

    let state = Arc::new(AppState {
        workspaces_root: data_dir.join("sessions"),
        user_profiles_dir: data_dir.join("profiles"),
        creds,
        public_url,
        run_permits: Semaphore::new(max_runs),
    });

    let static_dir = env_or("STATIC_DIR", "./dist");
    let spa = ServeDir::new(&static_dir)
        .fallback(ServeFile::new(PathBuf::from(&static_dir).join("index.html")));

    let app = Router::new()
        .route("/api/list_profiles", post(list_profiles))
        .route("/api/load_profile", post(load_profile))
        .route("/api/validate_file", post(validate_file))
        .route("/api/run_profile", post(run_profile))
        .route("/api/run_re_query", post(run_re_query))
        .route("/api/run_code_table_sync", post(run_code_table_sync))
        .route("/api/run_visualization", post(run_visualization))
        .route("/api/run_user_input", post(run_user_input))
        .route("/api/run_report", post(run_report))
        .route("/api/run_report_action", post(run_report_action))
        .route("/api/save_profile", post(save_profile))
        .route("/api/new_profile", post(new_profile))
        .route("/api/duplicate_profile", post(duplicate_profile))
        .route("/api/delete_profile", post(delete_profile))
        .route("/api/validate_profile", post(validate_profile))
        .route("/api/scaffold_missing", post(scaffold_missing))
        .route("/api/re_nxt_status", post(re_nxt_status))
        .route("/api/connect_re_nxt", post(connect_re_nxt))
        .route("/api/disconnect_re_nxt", post(disconnect_re_nxt))
        .route("/api/oauth/callback", get(oauth_callback))
        .route(
            "/api/sessions/{sid}/inputs",
            post(upload_input).layer(DefaultBodyLimit::max(200 * 1024 * 1024)),
        )
        .route("/api/sessions/{sid}/artifacts/{*artifact_id}", get(download_artifact))
        .route("/api/sessions/{sid}/assets/{*rel}", get(session_asset))
        .fallback_service(spa)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = env_or("BIND_ADDR", "0.0.0.0:8080");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("cannot bind {}: {}", addr, e));
    tracing::info!("listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("server error");
}
