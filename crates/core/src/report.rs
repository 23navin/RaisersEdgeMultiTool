// report.rs
//
// The report execution pipeline. Turns a loaded report profile + user parameter
// values into rendered-ready data:
//
//   param values ─▶ queries (RE call, via re_calls) ─▶ transforms (DuckDB SQL)
//                                                       ─▶ ResultSets ─▶ return
//
// The pipeline is real end-to-end (placeholder substitution + DuckDB execution).
// The RE call goes through a re_calls::Transport — Live (real SKY API) or Mock
// (fixture files) — chosen by the command layer. See REPORT_PROFILES.md. No app
// state is held between calls — actions re-run the pipeline to recompute input.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use duckdb::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::db::{self, ResultSet};
use crate::errors::AppError;
use crate::profile::LoadedProfile;
use crate::re_calls::{self, Transport};

// ── Result types returned to commands.rs ─────────────────────────────────────

// One query's resolved request + outcome — surfaced so the frontend (and tests)
// can confirm the param→request merge happened.
#[derive(Debug, Serialize)]
pub struct QueryDebug {
    pub id: String,
    pub call_ref: Option<String>,
    pub resolved_request: Value, // the request after {{param:...}} substitution
    pub row_count: usize,
}

#[derive(Debug, Serialize)]
pub struct ReportRunResult {
    // Keyed by TRANSFORM output label — exactly what a visualization's `data`
    // field binds to.
    pub data: HashMap<String, ResultSet>,
    pub queries: Vec<QueryDebug>,
    pub generated_at: String,
    pub mode: String, // "live" | "mock" — which transport produced this run
}

#[derive(Debug, Serialize)]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
}

// ── run_report ────────────────────────────────────────────────────────────────

pub fn run_report(
    loaded: &LoadedProfile,
    param_values: &HashMap<String, Value>,
    transport: &Transport,
    run_dir: &Path,
) -> Result<ReportRunResult, AppError> {
    let structure = &loaded.structure;

    // 1. Flatten parameter values (with structure defaults) into placeholder
    //    substitutions: {{param:id}} for scalars, {{param:id.key}} for objects.
    let subs = build_param_subs(loaded, param_values);

    // 2. `run_dir` is a fresh per-run directory minted by the caller
    //    (workspace.rs) for the query results (mock or live). Unique per run,
    //    not per report — two concurrent runs must never share one. Nothing is
    //    cleared here because nothing is ever reused; the session reaper
    //    reclaims it.
    fs::create_dir_all(run_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create report run dir: {}", e)))?;

    // 3. Pull any declared code tables — transforms reference them as
    //    {{codetable:Label}} alongside {{query:Label}}.
    let code_table_paths = crate::code_tables::fetch_all(loaded, transport, &run_dir)?;

    // 4. Run each query and stash its JSON where transforms expect it.
    let mut query_paths: HashMap<String, String> = HashMap::new();
    let mut query_debug: Vec<QueryDebug> = Vec::with_capacity(structure.queries.len());
    for q in &structure.queries {
        // The request sent to RE: the ad-hoc `template` when present, else the
        // `bind` (saved-query style). Params are substituted into either.
        let request = match &q.template {
            Some(tpl) => resolve_value(tpl, &subs),
            None => resolve_value(&bind_to_value(&q.bind), &subs),
        };

        let fixture_path = loaded
            .temp_dir
            .join("fixtures")
            .join(format!("{}.json", q.output));

        let json = re_calls::execute_query(
            transport,
            q.call_ref.as_deref(),
            q.template.is_some(),
            &request,
            &fixture_path,
        )?;

        let row_count = json.as_array().map(|a| a.len()).unwrap_or(0);

        let out_path = run_dir.join(format!("{}.json", q.output));
        fs::write(
            &out_path,
            serde_json::to_vec(&json)
                .map_err(|e| AppError::ParseError(format!("Cannot serialize query result: {}", e)))?,
        )
        .map_err(|e| AppError::IoError(format!("Cannot write query result: {}", e)))?;

        query_paths.insert(
            q.output.clone(),
            out_path.to_string_lossy().replace('\\', "/"),
        );
        query_debug.push(QueryDebug {
            id: q.id.clone(),
            call_ref: q.call_ref.clone(),
            resolved_request: request,
            row_count,
        });
    }

    // 4. Run each transform's SQL over the query results.
    let conn = Connection::open_in_memory().map_err(|e| AppError::SqlError(e.to_string()))?;
    let mut data: HashMap<String, ResultSet> = HashMap::new();
    for t in &structure.report_transforms {
        let sql_template = loaded
            .sql_files
            .get(&t.sql)
            .ok_or_else(|| AppError::ParseError(format!("SQL file '{}' not found in profile", t.sql)))?;

        // Substitute {{query:Label}} with each query result's JSON path, then
        // {{codetable:Label}} with each fetched code table's.
        let mut sql = sql_template.clone();
        for (label, path) in &query_paths {
            sql = sql.replace(&format!("{{{{query:{}}}}}", label), path);
        }
        sql = db::substitute_labeled_paths(&sql, db::KIND_CODETABLE, &code_table_paths);

        let result = db::query_to_result_set(&conn, &sql)?;
        data.insert(t.output.clone(), result);
    }

    Ok(ReportRunResult {
        data,
        queries: query_debug,
        generated_at: chrono::Local::now().to_rfc3339(),
        mode: transport.label().to_string(),
    })
}

// ── run_report_action ──────────────────────────────────────────────────────────
// Re-runs the pipeline (stateless), collects the ids from the action's input
// result set, then performs the write-back through the transport (live: POST a
// created query; mock: a stub). Returns a descriptive message.

pub fn run_report_action(
    loaded: &LoadedProfile,
    action_id: &str,
    param_values: &HashMap<String, Value>,
    transport: &Transport,
    run_dir: &Path,
) -> Result<ActionResult, AppError> {
    let action = loaded
        .structure
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(|| AppError::ParseError(format!("No action '{}' in report", action_id)))?;

    if let Some(r) = action.call_ref.as_deref() {
        if re_calls::lookup(r).is_none() {
            return Err(AppError::ParseError(format!("Unknown RE call '{}'", r)));
        }
    }

    let run = run_report(loaded, param_values, transport, run_dir)?;

    let input_label = action
        .input
        .as_deref()
        .ok_or_else(|| AppError::ParseError(format!("Action '{}' has no `input`", action_id)))?;
    let rs = run
        .data
        .get(input_label)
        .ok_or_else(|| AppError::ParseError(format!("Action input '{}' not produced", input_label)))?;

    // Collect the ids the action sends, from the `id_field` column (deduped,
    // non-empty, original order).
    let ids: Vec<Value> = match action.bind.get("id_field").and_then(|v| v.as_str()) {
        Some(field) => {
            let idx = rs.columns.iter().position(|c| c == field).ok_or_else(|| {
                AppError::ParseError(format!(
                    "Action id_field '{}' is not a column in '{}'",
                    field, input_label
                ))
            })?;
            let mut seen = std::collections::HashSet::new();
            rs.rows
                .iter()
                .filter_map(|row| row.get(idx))
                .filter(|v| !v.is_empty())
                .filter(|v| seen.insert((*v).clone()))
                .map(|v| Value::String(v.clone()))
                .collect()
        }
        None => Vec::new(),
    };
    let count = ids.len();

    // Resolve the new query's name ({{param:...}} + {{now}}).
    let mut subs = build_param_subs(loaded, param_values);
    subs.push(("{{now}}".to_string(), chrono::Local::now().to_rfc3339()));
    let name = action
        .bind
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| apply_subs(s, &subs))
        .unwrap_or_else(|| "Untitled query".to_string());

    // Build the QueryAdd body (see "API reference/query.yaml" → QueryAdd). RE has
    // no "save this list of ids" endpoint — a static query is criteria, so the
    // collected ids become a single OneOf filter on the id field. The action's
    // `bind` supplies the two environment-specific numbers:
    //   type_id            — the query type (e.g. 18 for Constituent)
    //   id_query_field_id  — the query field the ids belong to
    let type_id = action.bind.get("type_id").and_then(|v| v.as_i64()).ok_or_else(|| {
        AppError::ParseError(format!(
            "Action '{}' needs bind.type_id (the RE query type id) to create a query",
            action_id
        ))
    })?;
    let id_field_id = action
        .bind
        .get("id_query_field_id")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| {
            AppError::ParseError(format!(
                "Action '{}' needs bind.id_query_field_id (the RE query field id for \
                 the ids being saved) to create a query",
                action_id
            ))
        })?;

    let request = serde_json::json!({
        "name": name,
        "format": "Static",
        "type_id": type_id,
        "filter_fields": [{
            "query_field_id": id_field_id,
            "compare_type": "None",
            "operator": "OneOf",
            "filter_values": ids,
            "left_parenthesis": false,
            "right_parenthesis": false
        }]
    });
    let created = re_calls::create_query(
        transport,
        action.call_ref.as_deref(),
        action.template.is_some(),
        &request,
    )?;

    let suffix = match transport {
        Transport::Mock => " (mock)".to_string(),
        Transport::Live { .. } => created
            .get("id")
            .map(|id| format!(" (RE id: {})", id))
            .unwrap_or_default(),
    };

    Ok(ActionResult {
        ok: true,
        message: format!(
            "Created RE query \"{}\" with {} record(s).{}",
            name, count, suffix
        ),
    })
}

// ── helpers ────────────────────────────────────────────────────────────────────

// Build the (placeholder → value) list. Starts from each parameter's `default`,
// then overlays the supplied `param_values`. Object values expand into dotted
// sub-placeholders (e.g. a date_range yields `{{param:id.from}}`/`.to`).
fn build_param_subs(
    loaded: &LoadedProfile,
    param_values: &HashMap<String, Value>,
) -> Vec<(String, String)> {
    let mut values: HashMap<String, Value> = HashMap::new();
    for p in &loaded.structure.parameters {
        if let Some(d) = &p.default {
            values.insert(p.id.clone(), d.clone());
        }
    }
    for (k, v) in param_values {
        values.insert(k.clone(), v.clone());
    }

    let mut subs: Vec<(String, String)> = Vec::new();
    for (id, value) in &values {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    subs.push((format!("{{{{param:{}.{}}}}}", id, k), scalar_to_string(v)));
                }
            }
            other => {
                subs.push((format!("{{{{param:{}}}}}", id), scalar_to_string(other)));
            }
        }
    }
    subs
}

// Stringify a scalar JSON value without surrounding quotes (strings pass through
// raw; numbers/bools use their literal form; null becomes empty).
fn scalar_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub(crate) fn apply_subs(s: &str, subs: &[(String, String)]) -> String {
    let mut out = s.to_string();
    for (placeholder, value) in subs {
        if out.contains(placeholder.as_str()) {
            out = out.replace(placeholder.as_str(), value);
        }
    }
    out
}

// Recursively apply placeholder substitution to every string within a JSON value.
fn resolve_value(value: &Value, subs: &[(String, String)]) -> Value {
    match value {
        Value::String(s) => Value::String(apply_subs(s, subs)),
        Value::Array(arr) => Value::Array(arr.iter().map(|v| resolve_value(v, subs)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), resolve_value(v, subs)))
                .collect(),
        ),
        other => other.clone(),
    }
}

// A QueryRef.bind (HashMap) as a JSON object, so it can be resolved + echoed.
fn bind_to_value(bind: &HashMap<String, Value>) -> Value {
    Value::Object(bind.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;
    use crate::workspace;
    use std::path::PathBuf;

    fn load() -> LoadedProfile {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/src/gift_activity");
        profile::load_from_dir(&dir).expect("gift_activity should load")
    }

    fn run_dir() -> PathBuf {
        std::env::temp_dir()
            .join("report-test-runs")
            .join(workspace::unique_token())
    }

    fn params() -> HashMap<String, Value> {
        let mut p = HashMap::new();
        p.insert(
            "gift_dates".to_string(),
            serde_json::json!({ "from": "2026-05-01", "to": "2026-05-31" }),
        );
        p
    }

    #[test]
    fn run_report_produces_resultset() {
        let loaded = load();
        let res =
            run_report(&loaded, &params(), &Transport::Mock, &run_dir()).expect("run_report ok");

        assert_eq!(res.mode, "mock");
        let rs = res.data.get("ConstituentGifts").expect("ConstituentGifts present");
        assert_eq!(rs.rows.len(), 5);
        assert!(rs.columns.contains(&"constituent_name".to_string()));
        assert!(rs.columns.contains(&"gift_id".to_string()));

        // The date params were merged into the ad-hoc query definition's
        // Between filter (filter_values[0] = from, [1] = to).
        let q = &res.queries[0];
        assert_eq!(q.row_count, 5);
        assert_eq!(
            q.resolved_request["filter_fields"][0]["filter_values"][0],
            serde_json::json!("2026-05-01")
        );
    }

    #[test]
    fn run_action_counts_ids() {
        let loaded = load();
        let r =
            run_report_action(&loaded, "save_re_query", &params(), &Transport::Mock, &run_dir())
                .expect("action ok");
        assert!(r.ok);
        assert!(r.message.contains("5 record"), "message was: {}", r.message);
        assert!(r.message.contains("Report Gifts"));
        assert!(r.message.contains("(mock)"));
    }

    // Exercises the exact path the running app uses: load the PACKED built-in
    // (so fixtures must have been zipped), extract to a temp dir, then run the
    // pipeline against that temp dir — not the source folder.
    #[test]
    fn run_report_from_builtin() {
        let dest = std::env::temp_dir()
            .join("report-test-builtin")
            .join(workspace::unique_token());
        let loaded =
            profile::load_builtin_into("gift_activity.import", &dest).expect("builtin loads");
        assert_eq!(loaded.structure.kind.as_deref(), Some("report"));
        let res =
            run_report(&loaded, &params(), &Transport::Mock, &run_dir()).expect("run_report ok");
        assert_eq!(res.data.get("ConstituentGifts").unwrap().rows.len(), 5);
    }
}
