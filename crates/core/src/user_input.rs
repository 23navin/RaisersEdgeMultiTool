// ── user_input.rs ──────────────────────────────────────────────────────────────
// Collects values the uploaded files don't carry.
//
//   uploaded files ─▶ rows_sql ─▶ one row of controls per returned row
//                              ─▶ the user fills them in
//                              ─▶ JSON on disk
//                              ─▶ {{form:Label}} in later SQL steps
//
// The step is the one place a profile asks the operator a question mid-pipeline
// (semester start/end dates, a batch number, a gift type to apply). It is the
// third member of the upstream family that re_query and code_table_sync already
// belong to: an earlier step publishes a named result, later transforms declare
// it, and SQL reads it as {{kind:Label}}.
//
// Nothing is cached backend-side: every call recomputes the row set from the
// files on disk and rewrites the JSON with whatever the user has typed so far,
// so a re-run after an upstream change can never serve a stale row list.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value};

use crate::db::{self, SqlSources};
use crate::errors::AppError;
use crate::profile::{LoadedProfile, Step, UserInputField};

// The single row a form with no `rows_sql` collects. Empty rather than invented
// so the published JSON carries no key column the author didn't ask for.
const SINGLE_ROW_KEY: &str = "";

#[derive(Debug, Serialize)]
pub struct FormRow {
    // Identifies the row. The frontend keys its controls on this and sends the
    // values back under it; SQL joins on it as the `key` column.
    pub key: String,
    // What to show beside the controls — the key, plus any extra columns
    // rows_sql selected, already stringified for display.
    pub columns: Vec<String>,
    pub display: Vec<String>,
    // field id → the value in the published JSON (blank when unfilled).
    pub values: HashMap<String, String>,
    // Required fields on this row that are still blank.
    pub missing: Vec<String>,
    // Select fields on this row holding a value that is no longer one of the
    // field's options — the list it was picked from has changed underneath it
    // (an re_query re-run returning a different fund list, say). Kept rather
    // than silently cleared, so the operator sees what they chose and why it
    // stopped being valid.
    pub stale: Vec<String>,
}

// One choice on a select field. `value` is what gets published; `label` is what
// the operator reads. They're the same when options_sql selects one column.
#[derive(Debug, Clone, Serialize)]
pub struct FieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct UserInputResult {
    pub form_output: String, // the label later SQL uses as {{form:<label>}}
    pub artifact_id: String, // where the rows were written — abs path here, an opaque id by the time api.rs is done
    pub rows: Vec<FormRow>,
    pub row_count: usize,
    // field id → its choices, for select fields only. Resolved once per run and
    // shared by every row: a field's options describe the field, not the row.
    pub options: HashMap<String, Vec<FieldOption>>,
    // True when every required field on every row is filled and no selection has
    // gone stale. Downstream steps gate on this: a blank date would otherwise
    // join as NULL and quietly empty a column of the import file.
    pub complete: bool,
}

// Execute one user_input step: work out which rows need values, merge in what
// the user has supplied, and publish the result.
//
// `values` is key → field id → value, exactly as the form holds it. Unknown
// keys (a row that disappeared when the file was swapped) are ignored rather
// than resurrected — the row set always comes from the SQL, never from the
// client.
pub fn run_user_input(
    loaded: &LoadedProfile,
    step: &Step,
    file_paths: &HashMap<String, String>,
    sources: &SqlSources,
    values: &HashMap<String, HashMap<String, String>>,
    forms_dir: &Path,
) -> Result<UserInputResult, AppError> {
    let form_output = step.form_output.as_deref().ok_or_else(|| {
        AppError::ParseError(format!(
            "user_input step '{}' has no `form_output` naming its values",
            step.label
        ))
    })?;

    let fields = step.fields.as_deref().unwrap_or(&[]);
    if fields.is_empty() {
        return Err(AppError::ParseError(format!(
            "user_input step '{}' declares no `fields` to collect",
            step.label
        )));
    }

    // 1. The rows to collect values for. No rows_sql means one unkeyed row —
    //    the "just ask me these once" form.
    let (columns, key_idx, raw_rows) = match step.rows_sql.as_deref() {
        Some(name) => {
            let sql = loaded.sql_files.get(name).ok_or_else(|| {
                AppError::ParseError(format!(
                    "user_input step '{}': rows_sql file '{}' not found in profile",
                    step.label, name
                ))
            })?;
            let rows = db::select_rows(file_paths, sources, sql)?;
            if rows.columns.is_empty() {
                return Err(AppError::ParseError(format!(
                    "user_input step '{}': sql/{} returned no columns",
                    step.label, name
                )));
            }
            // Which column identifies a row. Named, or the first one selected.
            let key_idx = match step.key_column.as_deref() {
                Some(k) => rows.columns.iter().position(|c| c == k).ok_or_else(|| {
                    AppError::ParseError(format!(
                        "user_input step '{}': key_column '{}' is not one of the columns sql/{} returns ({})",
                        step.label,
                        k,
                        name,
                        rows.columns.join(", ")
                    ))
                })?,
                None => 0,
            };
            (rows.columns, key_idx, rows.rows)
        }
        None => (Vec::new(), 0, vec![Vec::new()]),
    };

    // 2. The choices for every select field. A static `options:` list is used
    //    as-is; `options_sql` is run against the same sources rows_sql saw, so
    //    a field can offer what an earlier step fetched.
    let mut options: HashMap<String, Vec<FieldOption>> = HashMap::new();
    for f in fields {
        if let Some(name) = f.options_sql.as_deref() {
            let sql = loaded.sql_files.get(name).ok_or_else(|| {
                AppError::ParseError(format!(
                    "user_input step '{}': field '{}' options_sql file '{}' not found in profile",
                    step.label, f.id, name
                ))
            })?;
            let result = db::select_rows(file_paths, sources, sql)?;
            if result.columns.is_empty() {
                return Err(AppError::ParseError(format!(
                    "user_input step '{}': field '{}' options_sql sql/{} returned no columns",
                    step.label, f.id, name
                )));
            }
            // First column is the value; a second, when present, is the label.
            let mut seen_values: HashSet<String> = HashSet::new();
            let choices: Vec<FieldOption> = result
                .rows
                .iter()
                .filter_map(|r| {
                    let value = r.first().map(|v| v.trim().to_string())?;
                    if value.is_empty() || !seen_values.insert(value.clone()) {
                        return None;
                    }
                    let label = r
                        .get(1)
                        .map(|l| l.trim())
                        .filter(|l| !l.is_empty())
                        .unwrap_or(&value)
                        .to_string();
                    Some(FieldOption { value, label })
                })
                .collect();
            options.insert(f.id.clone(), choices);
        } else if let Some(fixed) = f.options.as_ref() {
            options.insert(
                f.id.clone(),
                fixed
                    .iter()
                    .map(|o| FieldOption { value: o.clone(), label: o.clone() })
                    .collect(),
            );
        }
    }

    // 3. Merge the supplied values into each row. Duplicate keys collapse to
    //    one row: two controls writing the same key would publish two rows the
    //    author can't tell apart on a join.
    let mut seen: HashSet<String> = HashSet::new();
    let mut rows: Vec<FormRow> = Vec::new();
    let mut published: Vec<Value> = Vec::new();

    for raw in &raw_rows {
        let key = raw
            .get(key_idx)
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| SINGLE_ROW_KEY.to_string());
        if !seen.insert(key.clone()) {
            continue;
        }

        let supplied = values.get(&key);
        let mut obj = Map::new();
        // Every column rows_sql selected travels with the row, so SQL can join
        // on more than the key when the author selected more.
        for (i, col) in columns.iter().enumerate() {
            obj.insert(
                col.clone(),
                raw.get(i).map(|v| Value::String(v.clone())).unwrap_or(Value::Null),
            );
        }
        obj.insert("key".to_string(), Value::String(key.clone()));

        let mut values_out: HashMap<String, String> = HashMap::new();
        let mut missing: Vec<String> = Vec::new();
        let mut stale: Vec<String> = Vec::new();
        for f in fields {
            let v = resolve_value(f, supplied);
            match &v {
                Some(s) => {
                    // A held selection the field no longer offers. Published
                    // anyway — dropping it would hide the mismatch — but it
                    // keeps the step from counting as done.
                    if let Some(choices) = options.get(&f.id) {
                        if !choices.iter().any(|c| c.value == *s) {
                            stale.push(f.id.clone());
                        }
                    }
                    values_out.insert(f.id.clone(), s.clone());
                    obj.insert(f.id.clone(), Value::String(s.clone()));
                }
                None => {
                    // A blank field is published as null rather than "" so SQL
                    // can test it with IS NULL like any other missing value.
                    obj.insert(f.id.clone(), Value::Null);
                    if f.required {
                        missing.push(f.id.clone());
                    }
                }
            }
        }

        rows.push(FormRow {
            key,
            columns: columns.clone(),
            display: raw.clone(),
            values: values_out,
            missing,
            stale,
        });
        published.push(Value::Object(obj));
    }

    // 4. Write them where later SQL will read them. One stable filename per
    //    label — the file is the form's current state, so an edit overwrites.
    fs::create_dir_all(forms_dir)
        .map_err(|e| AppError::IoError(format!("Cannot create forms dir: {}", e)))?;
    let path: PathBuf = forms_dir.join(format!("form_{}.json", form_output));
    fs::write(&path, serde_json::to_vec(&published).unwrap_or_default())
        .map_err(|e| AppError::IoError(format!("Cannot write form values: {}", e)))?;

    let complete = rows
        .iter()
        .all(|r| r.missing.is_empty() && r.stale.is_empty());
    Ok(UserInputResult {
        form_output: form_output.to_string(),
        artifact_id: path.to_string_lossy().to_string(),
        row_count: rows.len(),
        options,
        complete,
        rows,
    })
}

// What one field holds on one row: whatever the user typed, or the declared
// default while the box is still untouched. A box the user cleared sends an
// empty string and stays empty — the default seeds a field, it doesn't fight
// the person editing it, and what the form shows always matches what was
// published.
fn resolve_value(
    field: &UserInputField,
    supplied: Option<&HashMap<String, String>>,
) -> Option<String> {
    if let Some(typed) = supplied.and_then(|m| m.get(&field.id)) {
        let t = typed.trim();
        return if t.is_empty() { None } else { Some(t.to_string()) };
    }
    field
        .default
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;

    fn field(id: &str, required: bool, default: Option<&str>) -> UserInputField {
        UserInputField {
            id: id.to_string(),
            label: id.to_string(),
            field_type: "date".to_string(),
            required,
            options: None,
            options_sql: None,
            default: default.map(|s| s.to_string()),
        }
    }

    // A step carrying only what run_user_input reads, borrowed from a real
    // bundle so the struct stays in step with profile.rs.
    fn step_with(fields: Vec<UserInputField>, rows_sql: Option<&str>) -> (LoadedProfile, Step) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/src/re_query_demo");
        let loaded = profile::load_from_dir(&dir).expect("demo loads");
        let mut step = loaded.structure.steps[0].clone();
        step.label = "Ask".to_string();
        step.step_type = "user_input".to_string();
        step.form_output = Some("Terms".to_string());
        step.fields = Some(fields);
        step.rows_sql = rows_sql.map(|s| s.to_string());
        step.key_column = None;
        (loaded, step)
    }

    fn forms_dir(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("form-test-{}", name))
            .join(crate::workspace::unique_token())
    }

    // No rows_sql: one unkeyed row, values merged in, published as one object.
    #[test]
    fn single_row_form_publishes_one_object() {
        let (loaded, step) = step_with(vec![field("date_from", true, None)], None);
        let values = HashMap::from([(
            String::new(),
            HashMap::from([("date_from".to_string(), "2025-09-22".to_string())]),
        )]);
        let dir = forms_dir("single");
        let res = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &values,
            &dir,
        )
        .expect("runs");
        assert_eq!(res.row_count, 1);
        assert!(res.complete);
        let written: Value =
            serde_json::from_slice(&fs::read(&res.artifact_id).unwrap()).unwrap();
        assert_eq!(written[0]["date_from"], "2025-09-22");
    }

    // A required field nobody filled leaves the step incomplete, and the blank
    // travels as null so SQL can test for it.
    #[test]
    fn missing_required_value_is_reported_and_published_as_null() {
        let (loaded, step) = step_with(vec![field("date_to", true, None)], None);
        let dir = forms_dir("missing");
        let res = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::new(),
            &dir,
        )
        .expect("runs");
        assert!(!res.complete);
        assert_eq!(res.rows[0].missing, vec!["date_to".to_string()]);
        let written: Value =
            serde_json::from_slice(&fs::read(&res.artifact_id).unwrap()).unwrap();
        assert!(written[0]["date_to"].is_null());
    }

    // A declared default stands in for an untouched box — but a box the user
    // cleared stays cleared, so the form and the published file never disagree.
    #[test]
    fn default_seeds_an_untouched_field_only() {
        let (loaded, step) = step_with(vec![field("date_from", true, Some("2025-01-01"))], None);
        let dir = forms_dir("default");
        let untouched = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::new(),
            &dir,
        )
        .expect("runs");
        assert!(untouched.complete);
        assert_eq!(untouched.rows[0].values["date_from"], "2025-01-01");

        let cleared = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::from([(
                String::new(),
                HashMap::from([("date_from".to_string(), String::new())]),
            )]),
            &dir,
        )
        .expect("runs");
        assert!(!cleared.complete);
        assert!(!cleared.rows[0].values.contains_key("date_from"));
    }

    // A static options list becomes the field's choices, and a value outside it
    // is published but marked stale so the step can't count as done.
    #[test]
    fn static_options_flag_a_value_they_do_not_offer() {
        let mut f = field("pick", true, None);
        f.field_type = "select".to_string();
        f.options = Some(vec!["A".to_string(), "B".to_string()]);
        let (loaded, step) = step_with(vec![f], None);
        let dir = forms_dir("options");

        let ok = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::from([(
                String::new(),
                HashMap::from([("pick".to_string(), "A".to_string())]),
            )]),
            &dir,
        )
        .expect("runs");
        assert!(ok.complete);
        assert_eq!(ok.options["pick"].len(), 2);
        assert_eq!(ok.options["pick"][0].label, "A");

        let gone = run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::from([(
                String::new(),
                HashMap::from([("pick".to_string(), "Z".to_string())]),
            )]),
            &dir,
        )
        .expect("runs");
        assert!(!gone.complete, "a value the field no longer offers is stale");
        assert_eq!(gone.rows[0].stale, vec!["pick".to_string()]);
        // Still published — hiding it would hide the mismatch.
        assert_eq!(gone.rows[0].values["pick"], "Z");
    }

    // A form with no fields collects nothing — a profile error, not an empty run.
    #[test]
    fn no_fields_is_an_error() {
        let (loaded, step) = step_with(vec![], None);
        let dir = forms_dir("nofields");
        assert!(run_user_input(
            &loaded,
            &step,
            &HashMap::new(),
            &SqlSources::new(),
            &HashMap::new(),
            &dir
        )
        .is_err());
    }
}
