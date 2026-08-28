// validate.rs
//
// Profile validator + missing-file scaffolder.
//
// Public entry points:
//   validate_profile(files)  -> ValidationReport
//   scaffold_missing(files)  -> Vec<ProfileFileEntry>
//
// Locations in issues are reported semantically (YamlStep("X"), MdAnchor("Y"),
// etc.) so the frontend can map them back to line numbers using the existing
// anchor map. The only line-bearing locations are YamlLine / MdLine / Sql{line}
// — those are emitted when an underlying parser hands us a row directly.

use std::collections::{HashMap, HashSet};
use duckdb::Connection;
use serde::Serialize;

use crate::profile::{
    InputDefinition, NoticeQuery, ProfileFileEntry, ProfileStructure, SqlTransform, Step,
    StepInputRef,
};

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IssueLocation {
    YamlStep { label: String },
    YamlInput { label: String },
    YamlOutput { label: String },
    YamlLine { line: usize },
    MdAnchor { label: String },
    MdLine { line: usize },
    Sql { path: String, line: Option<usize> },
    File { path: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationIssue {
    pub severity: Severity,
    pub code: String,
    pub message: String,
    pub location: Option<IssueLocation>,
    pub fixable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidationReport {
    pub issues: Vec<ValidationIssue>,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub fixable_count: usize,
}

// ── validate_profile ─────────────────────────────────────────────────────────

pub fn validate_profile(files: &[ProfileFileEntry]) -> ValidationReport {
    let mut issues: Vec<ValidationIssue> = Vec::new();

    let yaml_file = files.iter().find(|f| f.path == "structure.yaml");
    let md_file = files.iter().find(|f| f.path == "instructions.md");

    let yaml_content = match yaml_file {
        Some(f) => f.content.as_str(),
        None => {
            issues.push(err(
                "yaml.missing_file",
                "structure.yaml is missing from the bundle".to_string(),
                None,
                false,
            ));
            return finalize(issues);
        }
    };

    // Try to parse structure.yaml. A parse error is terminal — we can't
    // perform structural checks without a parsed model.
    let structure: ProfileStructure = match serde_yaml::from_str(yaml_content) {
        Ok(s) => s,
        Err(e) => {
            let line = e.location().map(|loc| loc.line());
            issues.push(ValidationIssue {
                severity: Severity::Error,
                code: "yaml.parse_failed".to_string(),
                message: format!("structure.yaml: {}", e),
                location: line.map(|l| IssueLocation::YamlLine { line: l }),
                fixable: false,
            });
            return finalize(issues);
        }
    };

    // Bundle-level: index sql/ files and instructions.md anchors.
    let sql_files: HashMap<String, &str> = files
        .iter()
        .filter(|f| f.path.starts_with("sql/") && f.path.ends_with(".sql"))
        .map(|f| (f.path["sql/".len()..].to_string(), f.content.as_str()))
        .collect();

    let md_anchors: HashMap<String, usize> = match md_file {
        Some(f) => parse_md_anchors(&f.content),
        None => HashMap::new(),
    };

    // Duplicate labels
    check_duplicates(
        &structure.steps.iter().map(|s| s.label.clone()).collect::<Vec<_>>(),
        "yaml.duplicate_step_label",
        |label| IssueLocation::YamlStep { label },
        &mut issues,
    );
    check_duplicates(
        &structure.inputs.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
        "yaml.duplicate_input_label",
        |label| IssueLocation::YamlInput { label },
        &mut issues,
    );
    check_duplicates(
        &structure.outputs.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
        "yaml.duplicate_output_label",
        |label| IssueLocation::YamlOutput { label },
        &mut issues,
    );

    let valid_input_labels: HashSet<&str> =
        structure.inputs.iter().map(|i| i.label.as_str()).collect();
    let valid_output_labels: HashSet<&str> =
        structure.outputs.iter().map(|o| o.label.as_str()).collect();

    // Track which SQL files are referenced so we can flag orphans.
    let mut referenced_sql: HashSet<String> = HashSet::new();

    // ── code_tables section ──────────────────────────────────────────────────
    // Shared by both profile kinds. Each entry must say which RE table it means
    // and expose a unique label for SQL's {{codetable:Label}} placeholder.
    {
        let mut seen_outputs: HashSet<&str> = HashSet::new();
        for ct in &structure.code_tables {
            if ct.name.is_none() && ct.code_table_id.is_none() {
                issues.push(err(
                    "yaml.code_tables.no_table",
                    format!(
                        "code_tables entry '{}' needs `name` (exact code table name) or `code_table_id`",
                        ct.id
                    ),
                    None,
                    false,
                ));
            }
            if ct.output.trim().is_empty() {
                issues.push(err(
                    "yaml.code_tables.no_output",
                    format!("code_tables entry '{}' needs a non-empty `output` label", ct.id),
                    None,
                    false,
                ));
            } else if !seen_outputs.insert(ct.output.as_str()) {
                issues.push(err(
                    "yaml.code_tables.duplicate_output",
                    format!(
                        "code_tables output '{}' is declared more than once — {{{{codetable:{}}}}} would be ambiguous",
                        ct.output, ct.output
                    ),
                    None,
                    false,
                ));
            }
        }
    }

    // Every {{codetable:Label}} a SQL file uses must name a declared table.
    {
        let declared: HashSet<&str> = structure
            .code_tables
            .iter()
            .map(|c| c.output.as_str())
            .collect();
        for (file, content) in &sql_files {
            for label in labeled_placeholders(content, "codetable") {
                if !declared.contains(label.as_str()) {
                    issues.push(err(
                        "sql.unknown_code_table",
                        format!(
                            "sql/{} references {{{{codetable:{}}}}} but no code_tables entry declares that output{}",
                            file,
                            label,
                            if declared.is_empty() {
                                " (the profile declares none)".to_string()
                            } else {
                                format!(" (declared: {})", declared.iter().cloned().collect::<Vec<_>>().join(", "))
                            }
                        ),
                        Some(IssueLocation::Sql { path: format!("sql/{}", file), line: None }),
                        false,
                    ));
                }
            }
        }
    }

    // ── Upstream wiring (query_output/query_input, sync_output/sync_input) ───
    // Both families share one rule: an earlier step produces a label, a later
    // transform declares it, and SQL reads it as {{kind:Label}}. Checked by one
    // routine so a third family is a table entry rather than another 100 lines.
    for family in UPSTREAM_FAMILIES {
        check_upstream_wiring(&structure, &sql_files, family, &mut issues);
    }

    // ── Per-step structural checks ───────────────────────────────────────────
    for step in &structure.steps {
        let step_loc = || IssueLocation::YamlStep { label: step.label.clone() };

        match step.step_type.as_str() {
            "file_input" => {
                let inputs = step.input.as_deref().unwrap_or(&[]);
                if inputs.is_empty() {
                    issues.push(err(
                        "yaml.file_input.no_inputs",
                        format!("Step '{}' (file_input) declares no inputs", step.label),
                        Some(step_loc()),
                        false,
                    ));
                }
                for r in inputs {
                    let lbl = ref_label(r);
                    if !valid_input_labels.contains(lbl.as_str()) {
                        issues.push(err(
                            "yaml.input_ref.undeclared",
                            format!(
                                "Step '{}' references undeclared input '{}'",
                                step.label, lbl
                            ),
                            Some(step_loc()),
                            false,
                        ));
                    }
                }
            }

            "sql_transform" => {
                let has_transforms = step.transforms.as_ref().is_some_and(|t| !t.is_empty());
                let has_shortcut =
                    step.sql.is_some() || step.input.is_some() || step.output.is_some();
                if has_transforms && has_shortcut {
                    issues.push(err(
                        "yaml.sql_transform.ambiguous_shape",
                        format!(
                            "Step '{}' uses both `transforms:` and the shortcut fields. Pick one.",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    ));
                }

                // Iterate transform units uniformly.
                let units: Vec<TransformUnit> = if let Some(ts) = &step.transforms {
                    ts.iter()
                        .map(|t| TransformUnit {
                            input: t.input.as_deref().unwrap_or(&[]),
                            sql: t.sql.as_str(),
                            output: t.output.as_deref().unwrap_or(&[]),
                            notices_sql: t
                                .notices
                                .as_deref()
                                .unwrap_or(&[])
                                .iter()
                                .map(|n| n.sql.clone())
                                .collect(),
                        })
                        .collect()
                } else {
                    vec![TransformUnit {
                        input: step.input.as_deref().unwrap_or(&[]),
                        sql: step.sql.as_deref().unwrap_or(""),
                        output: step.output.as_deref().unwrap_or(&[]),
                        notices_sql: step
                            .notices
                            .as_deref()
                            .unwrap_or(&[])
                            .iter()
                            .map(|n| n.sql.clone())
                            .collect(),
                    }]
                };

                for unit in &units {
                    if unit.input.is_empty() {
                        issues.push(err(
                            "yaml.sql_transform.no_inputs",
                            format!("Step '{}' has a transform with no inputs", step.label),
                            Some(step_loc()),
                            false,
                        ));
                    }
                    if unit.output.is_empty() {
                        issues.push(err(
                            "yaml.sql_transform.no_outputs",
                            format!("Step '{}' has a transform with no outputs", step.label),
                            Some(step_loc()),
                            false,
                        ));
                    }

                    for r in unit.input {
                        let lbl = ref_label(r);
                        if !valid_input_labels.contains(lbl.as_str()) {
                            issues.push(err(
                                "yaml.input_ref.undeclared",
                                format!(
                                    "Step '{}' references undeclared input '{}'",
                                    step.label, lbl
                                ),
                                Some(step_loc()),
                                false,
                            ));
                        }
                    }
                    for o in unit.output {
                        if !valid_output_labels.contains(o.as_str()) {
                            issues.push(err(
                                "yaml.output_ref.undeclared",
                                format!(
                                    "Step '{}' references undeclared output '{}'",
                                    step.label, o
                                ),
                                Some(step_loc()),
                                false,
                            ));
                        }
                    }

                    // Main sql file presence (fixable via scaffold).
                    if unit.sql.is_empty() {
                        issues.push(err(
                            "yaml.sql_transform.missing_sql_field",
                            format!("Step '{}' has a transform with no `sql:` field", step.label),
                            Some(step_loc()),
                            false,
                        ));
                    } else {
                        referenced_sql.insert(unit.sql.to_string());
                        if !sql_files.contains_key(unit.sql) {
                            issues.push(err(
                                "yaml.sql_transform.missing_sql_file",
                                format!(
                                    "Step '{}' references sql/{} but the file is missing",
                                    step.label, unit.sql
                                ),
                                Some(step_loc()),
                                true, // scaffoldable
                            ));
                        } else {
                            check_sql_placeholders(
                                unit,
                                sql_files.get(unit.sql).unwrap(),
                                &valid_input_labels,
                                &valid_output_labels,
                                &mut issues,
                            );
                        }
                    }

                    // Notice SQL presence.
                    for n in &unit.notices_sql {
                        referenced_sql.insert(n.clone());
                        if !sql_files.contains_key(n) {
                            issues.push(err(
                                "yaml.notices.missing_sql_file",
                                format!(
                                    "Step '{}' notice references sql/{} but the file is missing",
                                    step.label, n
                                ),
                                Some(step_loc()),
                                true, // scaffoldable
                            ));
                        }
                    }
                }
            }

            "manual_instruction" => {
                if !md_anchors.contains_key(&step.label) {
                    issues.push(err(
                        "md.manual_instruction.missing_anchor",
                        format!(
                            "manual_instruction step '{}' has no <!-- label: ... --> anchor in instructions.md",
                            step.label
                        ),
                        Some(IssueLocation::MdAnchor { label: step.label.clone() }),
                        true,
                    ));
                }
            }

            "code_table_sync" => {
                // Which table: name or id, exactly one required.
                if step.code_table.is_none() && step.code_table_id.is_none() {
                    issues.push(err(
                        "yaml.code_table_sync.no_table",
                        format!(
                            "code_table_sync step '{}' needs `code_table` (name) or `code_table_id`",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    ));
                }

                // Which write, and is it one we support.
                match step.operation.as_deref() {
                    None => issues.push(err(
                        "yaml.code_table_sync.no_operation",
                        format!(
                            "code_table_sync step '{}' needs `operation` (create, update, or delete)",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    )),
                    Some(op) if !matches!(op, "create" | "update" | "delete") => {
                        issues.push(err(
                            "yaml.code_table_sync.bad_operation",
                            format!(
                                "code_table_sync step '{}' has operation '{}'; expected create, update, or delete",
                                step.label, op
                            ),
                            Some(step_loc()),
                            false,
                        ))
                    }
                    Some(_) => {}
                }

                // The rows to push come from a SQL file that must exist.
                match step.sql.as_deref() {
                    None => issues.push(err(
                        "yaml.code_table_sync.missing_sql_field",
                        format!(
                            "code_table_sync step '{}' has no `sql` naming the rows to push",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    )),
                    Some(sql_name) => {
                        if !sql_files.contains_key(sql_name) {
                            issues.push(err(
                                "yaml.code_table_sync.missing_sql_file",
                                format!(
                                    "code_table_sync step '{}' references missing file sql/{}",
                                    step.label, sql_name
                                ),
                                Some(IssueLocation::Sql { path: format!("sql/{}", sql_name), line: None }),
                                true,
                            ));
                        } else if step.operation.as_deref() == Some("create") {
                            // create needs a long_description column; a query that
                            // clearly can't produce one is worth flagging early.
                            let sql = sql_files.get(sql_name).map(|s| s.to_lowercase()).unwrap_or_default();
                            if !sql.contains("long_description") {
                                issues.push(warning(
                                    "sql.code_table_sync.no_long_description",
                                    format!(
                                        "code_table_sync step '{}' creates entries but sql/{} never mentions long_description (the one required field)",
                                        step.label, sql_name
                                    ),
                                    Some(IssueLocation::Sql { path: format!("sql/{}", sql_name), line: None }),
                                    false,
                                ));
                            }
                        }
                    }
                }
            }

            "re_query" => {
                // Something runnable: a registry ref or an inline template.
                if step.call_ref.is_none() && step.template.is_none() {
                    issues.push(err(
                        "yaml.re_query.no_call",
                        format!(
                            "re_query step '{}' needs a `ref` (registry call) or an inline `template`",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    ));
                }
                // The label later SQL will reference.
                match step.query_output.as_deref() {
                    None => issues.push(err(
                        "yaml.re_query.no_output",
                        format!(
                            "re_query step '{}' needs `query_output` naming its result",
                            step.label
                        ),
                        Some(step_loc()),
                        false,
                    )),
                    Some(out) if out.trim().is_empty() => issues.push(err(
                        "yaml.re_query.no_output",
                        format!("re_query step '{}' has an empty `query_output`", step.label),
                        Some(step_loc()),
                        false,
                    )),
                    Some(_) => {}
                }
                // params_sql is optional, but must exist when named.
                if let Some(name) = step.params_sql.as_deref() {
                    if !sql_files.contains_key(name) {
                        issues.push(err(
                            "yaml.re_query.missing_params_sql",
                            format!(
                                "re_query step '{}' references missing file sql/{}",
                                step.label, name
                            ),
                            Some(IssueLocation::Sql { path: format!("sql/{}", name), line: None }),
                            true,
                        ));
                    }
                }
            }

            other => {
                issues.push(err(
                    "yaml.unknown_step_type",
                    format!(
                        "Step '{}' has unknown type '{}'. Expected file_input, sql_transform, re_query, code_table_sync, or manual_instruction.",
                        step.label, other
                    ),
                    Some(step_loc()),
                    false,
                ));
            }
        }
    }

    // ── Cross-file: MD anchors vs YAML steps ─────────────────────────────────
    let step_labels: HashSet<&str> =
        structure.steps.iter().map(|s| s.label.as_str()).collect();

    for (anchor, line) in &md_anchors {
        if !step_labels.contains(anchor.as_str()) {
            issues.push(warning(
                "md.orphan_anchor",
                format!(
                    "instructions.md anchor '{}' doesn't correspond to any YAML step",
                    anchor
                ),
                Some(IssueLocation::MdLine { line: *line }),
                false,
            ));
        }
    }

    // Non-manual steps without an MD anchor: warning (fixable).
    for step in &structure.steps {
        if step.step_type == "manual_instruction" {
            continue; // already errored above if missing
        }
        if !md_anchors.contains_key(&step.label) {
            issues.push(warning(
                "md.step_missing_anchor",
                format!(
                    "Step '{}' has no <!-- label: {} --> section in instructions.md",
                    step.label, step.label
                ),
                Some(IssueLocation::YamlStep { label: step.label.clone() }),
                true,
            ));
        }
    }

    // Anchor order mismatch.
    let yaml_order: Vec<&str> = structure
        .steps
        .iter()
        .map(|s| s.label.as_str())
        .filter(|l| md_anchors.contains_key(*l))
        .collect();
    let mut md_order: Vec<(&str, usize)> = md_anchors
        .iter()
        .filter(|(k, _)| step_labels.contains(k.as_str()))
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    md_order.sort_by_key(|(_, line)| *line);
    let md_only: Vec<&str> = md_order.iter().map(|(k, _)| *k).collect();
    if yaml_order != md_only && !yaml_order.is_empty() {
        issues.push(warning(
            "md.anchor_order_mismatch",
            "instructions.md anchor order doesn't match the YAML step order".to_string(),
            Some(IssueLocation::File { path: "instructions.md".to_string() }),
            false,
        ));
    }

    // Orphan SQL files.
    for path in sql_files.keys() {
        if !referenced_sql.contains(path) {
            issues.push(warning(
                "sql.orphan_file",
                format!("sql/{} is not referenced by any step in structure.yaml", path),
                Some(IssueLocation::File { path: format!("sql/{}", path) }),
                false,
            ));
        }
    }

    // step.input.validate: true on an input without validation rules.
    for step in &structure.steps {
        if step.step_type != "file_input" {
            continue;
        }
        for r in step.input.as_deref().unwrap_or(&[]) {
            if let StepInputRef::Detailed(d) = r {
                if d.validate == Some(true) {
                    let input_def = structure.inputs.iter().find(|i| i.label == d.label);
                    let no_rules = match input_def {
                        Some(i) => i.validation.as_deref().map(|v| v.is_empty()).unwrap_or(true),
                        None => true,
                    };
                    if no_rules {
                        issues.push(warning(
                            "yaml.validate_without_columns",
                            format!(
                                "Input '{}' is set to validate but has no `validation:` columns declared",
                                d.label
                            ),
                            Some(IssueLocation::YamlInput { label: d.label.clone() }),
                            false,
                        ));
                    }
                }
            }
        }
    }

    // ── SQL compile checks ───────────────────────────────────────────────────
    // For each transform's SQL file, build subqueries from input validation
    // columns and ask DuckDB to plan the statement. We can only do this when
    // every referenced input has a known column shape — otherwise we'd be
    // making up columns the user's SQL doesn't actually project.
    let input_lookup: HashMap<&str, &InputDefinition> = structure
        .inputs
        .iter()
        .map(|i| (i.label.as_str(), i))
        .collect();

    for step in &structure.steps {
        if step.step_type != "sql_transform" {
            continue;
        }
        let units: Vec<TransformUnit> = if let Some(ts) = &step.transforms {
            ts.iter()
                .map(|t| TransformUnit {
                    input: t.input.as_deref().unwrap_or(&[]),
                    sql: t.sql.as_str(),
                    output: t.output.as_deref().unwrap_or(&[]),
                    notices_sql: Vec::new(),
                })
                .collect()
        } else {
            vec![TransformUnit {
                input: step.input.as_deref().unwrap_or(&[]),
                sql: step.sql.as_deref().unwrap_or(""),
                output: step.output.as_deref().unwrap_or(&[]),
                notices_sql: Vec::new(),
            }]
        };

        for unit in &units {
            let sql_filename = unit.sql;
            if sql_filename.is_empty() {
                continue;
            }
            let Some(sql_content) = sql_files.get(sql_filename) else {
                continue; // already reported as missing
            };

            // Bail out of the compile check if any referenced input has no
            // validation columns — surface as a skipped warning instead.
            let mut skipped = false;
            for r in unit.input {
                let lbl = ref_label(r);
                let Some(def) = input_lookup.get(lbl.as_str()) else { continue };
                let has_cols = def
                    .validation
                    .as_deref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
                if !has_cols {
                    issues.push(warning(
                        "sql.compile_skipped",
                        format!(
                            "sql/{} can't be compile-checked because input '{}' has no validation columns",
                            sql_filename, lbl
                        ),
                        Some(IssueLocation::Sql {
                            path: format!("sql/{}", sql_filename),
                            line: None,
                        }),
                        false,
                    ));
                    skipped = true;
                    break;
                }
            }
            if skipped {
                continue;
            }

            // Build the synth substitutions.
            let mut subqueries: HashMap<String, String> = HashMap::new();
            for r in unit.input {
                let lbl = ref_label(r);
                if let Some(def) = input_lookup.get(lbl.as_str()) {
                    subqueries.insert(lbl.clone(), input_subquery(def));
                }
            }
            let substituted = compile_substitute(sql_content, &subqueries);

            // Run each statement through DuckDB EXPLAIN. The EXPLAIN wrapper
            // keeps DuckDB in plan-only mode for both SELECT and COPY shapes.
            if let Err((stmt_idx, msg)) = explain_each(&substituted) {
                issues.push(err(
                    "sql.parse_failed",
                    format!(
                        "sql/{} statement #{} doesn't parse: {}",
                        sql_filename,
                        stmt_idx + 1,
                        msg
                    ),
                    Some(IssueLocation::Sql {
                        path: format!("sql/{}", sql_filename),
                        line: None,
                    }),
                    false,
                ));
            }
        }
    }

    // Info: scaffold available if anything is fixable.
    let any_fixable = issues.iter().any(|i| i.fixable);
    if any_fixable {
        issues.push(ValidationIssue {
            severity: Severity::Info,
            code: "scaffold.available".to_string(),
            message: "Some issues can be auto-fixed with the Scaffold action".to_string(),
            location: None,
            fixable: false,
        });
    }

    finalize(issues)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

struct TransformUnit<'a> {
    input: &'a [StepInputRef],
    sql: &'a str,
    output: &'a [String],
    notices_sql: Vec<String>,
}

fn ref_label(r: &StepInputRef) -> String {
    match r {
        StepInputRef::Simple(s) => s.clone(),
        StepInputRef::Detailed(d) => d.label.clone(),
    }
}

// Pull the labels out of every {{<kind>:Label}} in a SQL file — used for
// "codetable" and "query". Comments are not stripped: a commented-out
// placeholder naming a real label is harmless, and one naming a missing label is
// still worth surfacing.
fn labeled_placeholders(sql: &str, kind: &str) -> Vec<String> {
    let opener = format!("{{{{{}:", kind);
    let mut out = Vec::new();
    let mut rest = sql;
    while let Some(start) = rest.find(&opener) {
        let after = &rest[start + opener.len()..];
        match after.find("}}") {
            Some(end) => {
                out.push(after[..end].trim().to_string());
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out
}

// ── Upstream families ─────────────────────────────────────────────────────────
// A "family" is one producer-step-type publishing a named result that later
// transforms declare and SQL reads as {{kind:Label}}. Adding a new one means
// adding a row here and the matching accessors — no new validation logic.

struct UpstreamFamily {
    producer_type: &'static str, // step type that publishes a label
    kind: &'static str,          // placeholder kind: {{kind:Label}}
    output_field: &'static str,  // YAML key naming the published label
    input_field: &'static str,   // YAML key declaring a dependency on one
    dup_code: &'static str,
    unresolved_code: &'static str,
    sql_code: &'static str,
    // Plain fn pointers — neither accessor captures anything.
    output_of: fn(&Step) -> Option<&str>,
    inputs_of: fn(&Step) -> Vec<&String>,
}

const UPSTREAM_FAMILIES: &[UpstreamFamily] = &[
    UpstreamFamily {
        producer_type: "re_query",
        kind: "query",
        output_field: "query_output",
        input_field: "query_input",
        dup_code: "yaml.re_query.duplicate_output",
        unresolved_code: "yaml.query_input.unresolved",
        sql_code: "sql.unknown_query",
        output_of: |s| s.query_output.as_deref(),
        inputs_of: |s| collect_declared(s.query_input.as_ref(), s, |t| t.query_input.as_ref()),
    },
    UpstreamFamily {
        producer_type: "code_table_sync",
        kind: "sync",
        output_field: "sync_output",
        input_field: "sync_input",
        dup_code: "yaml.code_table_sync.duplicate_output",
        unresolved_code: "yaml.sync_input.unresolved",
        sql_code: "sql.unknown_sync",
        output_of: |s| s.sync_output.as_deref(),
        inputs_of: |s| collect_declared(s.sync_input.as_ref(), s, |t| t.sync_input.as_ref()),
    },
];

// A transform declares its dependencies either at step level (single-transform
// shortcut) or per entry in `transforms:`. Both shapes count.
fn collect_declared<'a>(
    step_level: Option<&'a Vec<String>>,
    step: &'a Step,
    per_transform: fn(&'a SqlTransform) -> Option<&'a Vec<String>>,
) -> Vec<&'a String> {
    let mut out: Vec<&String> = Vec::new();
    if let Some(v) = step_level {
        out.extend(v.iter());
    }
    if let Some(transforms) = &step.transforms {
        for t in transforms {
            if let Some(v) = per_transform(t) {
                out.extend(v.iter());
            }
        }
    }
    out
}

// Walk the steps in declaration order, accumulating the labels produced so far.
// A dependency may only name one already produced — steps run in the order the
// user clicks them, so a forward reference would read a result that doesn't
// exist yet.
fn check_upstream_wiring(
    structure: &ProfileStructure,
    sql_files: &HashMap<String, &str>,
    fam: &UpstreamFamily,
    issues: &mut Vec<ValidationIssue>,
) {
    let mut all_outputs: HashSet<&str> = HashSet::new();
    for step in &structure.steps {
        if step.step_type != fam.producer_type {
            continue;
        }
        if let Some(out) = (fam.output_of)(step) {
            if !out.trim().is_empty() && !all_outputs.insert(out) {
                issues.push(err(
                    fam.dup_code,
                    format!(
                        "{} '{}' is declared by more than one step — {{{{{}:{}}}}} would be ambiguous",
                        fam.output_field, out, fam.kind, out
                    ),
                    Some(IssueLocation::YamlStep { label: step.label.clone() }),
                    false,
                ));
            }
        }
    }

    let mut produced: HashSet<&str> = HashSet::new();
    for step in &structure.steps {
        for label in (fam.inputs_of)(step) {
            if produced.contains(label.as_str()) {
                continue;
            }
            let msg = if all_outputs.contains(label.as_str()) {
                format!(
                    "Step '{}' reads {} '{}' but the {} step producing it comes later — reorder the steps",
                    step.label, fam.kind, label, fam.producer_type
                )
            } else {
                format!(
                    "Step '{}' declares {} '{}' but no {} step produces it{}",
                    step.label,
                    fam.input_field,
                    label,
                    fam.producer_type,
                    if all_outputs.is_empty() {
                        format!(" (the profile declares no {} outputs)", fam.output_field)
                    } else {
                        format!(
                            " (available: {})",
                            all_outputs.iter().cloned().collect::<Vec<_>>().join(", ")
                        )
                    }
                )
            };
            issues.push(err(
                fam.unresolved_code,
                msg,
                Some(IssueLocation::YamlStep { label: step.label.clone() }),
                false,
            ));
        }

        // This step's own output becomes available to everything after it.
        if step.step_type == fam.producer_type {
            if let Some(out) = (fam.output_of)(step) {
                produced.insert(out);
            }
        }
    }

    // Every {{kind:Label}} in a SQL file must name some declared output.
    for (file, content) in sql_files {
        for label in labeled_placeholders(content, fam.kind) {
            if !all_outputs.contains(label.as_str()) {
                issues.push(err(
                    fam.sql_code,
                    format!(
                        "sql/{} references {{{{{}:{}}}}} but no {} step declares that {}{}",
                        file,
                        fam.kind,
                        label,
                        fam.producer_type,
                        fam.output_field,
                        if all_outputs.is_empty() {
                            " (the profile declares none)".to_string()
                        } else {
                            format!(
                                " (declared: {})",
                                all_outputs.iter().cloned().collect::<Vec<_>>().join(", ")
                            )
                        }
                    ),
                    Some(IssueLocation::Sql { path: format!("sql/{}", file), line: None }),
                    false,
                ));
            }
        }
    }
}

fn err(code: &str, msg: String, loc: Option<IssueLocation>, fixable: bool) -> ValidationIssue {
    ValidationIssue {
        severity: Severity::Error,
        code: code.to_string(),
        message: msg,
        location: loc,
        fixable,
    }
}

fn warning(code: &str, msg: String, loc: Option<IssueLocation>, fixable: bool) -> ValidationIssue {
    ValidationIssue {
        severity: Severity::Warning,
        code: code.to_string(),
        message: msg,
        location: loc,
        fixable,
    }
}

fn finalize(issues: Vec<ValidationIssue>) -> ValidationReport {
    let mut e = 0;
    let mut w = 0;
    let mut i = 0;
    let mut f = 0;
    for issue in &issues {
        match issue.severity {
            Severity::Error => e += 1,
            Severity::Warning => w += 1,
            Severity::Info => i += 1,
        }
        if issue.fixable {
            f += 1;
        }
    }
    ValidationReport {
        issues,
        error_count: e,
        warning_count: w,
        info_count: i,
        fixable_count: f,
    }
}

fn check_duplicates(
    labels: &[String],
    code: &str,
    mut loc_for: impl FnMut(String) -> IssueLocation,
    issues: &mut Vec<ValidationIssue>,
) {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut reported: HashSet<&str> = HashSet::new();
    for l in labels {
        if !seen.insert(l.as_str()) && reported.insert(l.as_str()) {
            issues.push(err(
                code,
                format!("'{}' is declared more than once", l),
                Some(loc_for(l.clone())),
                false,
            ));
        }
    }
}

fn parse_md_anchors(md: &str) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for (i, line) in md.lines().enumerate() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("<!-- label:") {
            if let Some(inner) = rest.strip_suffix("-->") {
                let label = inner.trim().to_string();
                if !label.is_empty() {
                    out.entry(label).or_insert(i + 1);
                }
            }
        }
    }
    out
}

// Scan an SQL file for {{input:X}} / {{output:X}} / {{input_file}} usage and
// emit issues for placeholders that don't match the owning transform.
fn check_sql_placeholders(
    unit: &TransformUnit,
    sql_content: &str,
    all_inputs: &HashSet<&str>,
    _all_outputs: &HashSet<&str>,
    issues: &mut Vec<ValidationIssue>,
) {
    let declared_inputs: HashSet<&str> =
        unit.input.iter().map(|r| match r {
            StepInputRef::Simple(s) => s.as_str(),
            StepInputRef::Detailed(d) => d.label.as_str(),
        }).collect();
    let declared_outputs: HashSet<&str> =
        unit.output.iter().map(|s| s.as_str()).collect();

    let sql_path = format!("sql/{}", unit.sql);

    let mut output_placeholder_count = 0usize;
    for (kind, name) in scan_placeholders(sql_content) {
        match kind.as_str() {
            "input" => {
                if !declared_inputs.contains(name.as_str()) {
                    let in_profile = all_inputs.contains(name.as_str());
                    let msg = if in_profile {
                        format!(
                            "{{{{input:{}}}}}  is not in this transform's declared inputs",
                            name
                        )
                    } else {
                        format!(
                            "{{{{input:{}}}}} references an input that isn't declared in structure.yaml",
                            name
                        )
                    };
                    issues.push(err(
                        "sql.placeholder_input.undeclared",
                        msg,
                        Some(IssueLocation::Sql { path: sql_path.clone(), line: None }),
                        false,
                    ));
                }
            }
            "output" => {
                output_placeholder_count += 1;
                if !declared_outputs.contains(name.as_str()) {
                    issues.push(err(
                        "sql.placeholder_output.undeclared",
                        format!(
                            "{{{{output:{}}}}} is not in this transform's declared outputs",
                            name
                        ),
                        Some(IssueLocation::Sql { path: sql_path.clone(), line: None }),
                        false,
                    ));
                }
            }
            _ => {}
        }
    }

    // {{input_file}} legacy alias: only valid with exactly one input.
    if sql_content.contains("{{input_file}}") && unit.input.len() > 1 {
        issues.push(err(
            "sql.input_file_multi",
            format!(
                "sql/{} uses {{{{input_file}}}} but the transform has {} inputs. Use {{{{input:Label}}}} placeholders instead.",
                unit.sql, unit.input.len()
            ),
            Some(IssueLocation::Sql { path: sql_path.clone(), line: None }),
            false,
        ));
    }

    // Output shape mismatch:
    //   - >1 declared outputs but no {{output:X}} placeholders → error (the
    //     runtime requires multi-output mode to be opt-in via placeholders).
    //   - 1 declared output but multiple {{output:X}} placeholders → covered
    //     above as undeclared if names mismatch; if names match the single
    //     output, the runtime still works.
    if unit.output.len() > 1 && output_placeholder_count == 0 {
        issues.push(err(
            "sql.output_mismatch",
            format!(
                "sql/{} has {} declared outputs but no {{{{output:Label}}}} placeholders. Add COPY (...) TO '{{{{output:Label}}}}' statements.",
                unit.sql, unit.output.len()
            ),
            Some(IssueLocation::Sql { path: sql_path, line: None }),
            false,
        ));
    }
}

// Find every {{kind:name}} placeholder (kind ∈ {input, output}).
fn scan_placeholders(sql: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = sql;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let inner = &after[..end];
        if let Some(colon) = inner.find(':') {
            let kind = inner[..colon].trim().to_string();
            let name = inner[colon + 1..].trim().to_string();
            if !name.is_empty() && (kind == "input" || kind == "output") {
                out.push((kind, name));
            }
        }
        rest = &after[end + 2..];
    }
    out
}

// Build a `(SELECT NULL::TYPE AS "Col", ...)` subquery from an input's
// declared validation columns. Returns the empty-table marker when the input
// has no columns — callers should skip the compile check before getting here,
// but we still emit something parseable to avoid bringing the whole batch
// down on a single missing input.
fn input_subquery(input: &InputDefinition) -> String {
    let cols: Vec<String> = input
        .validation
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .map(|v| {
            let ty = if v.col_type == "number" { "DOUBLE" } else { "VARCHAR" };
            format!("NULL::{} AS \"{}\"", ty, v.label)
        })
        .collect();
    if cols.is_empty() {
        "(SELECT NULL AS _empty WHERE FALSE)".to_string()
    } else {
        format!("(SELECT {})", cols.join(", "))
    }
}

// Replace `read_csv_auto('{{input:X}}')`, `read_xlsx('{{input:X}}')`, or the
// legacy single-input `read_csv_auto('{{input_file}}')` form with the synth
// subquery for that input. Any `{{output:Y}}` outside a read_*() call becomes
// a throwaway string path so COPY statements parse.
//
// Input placeholders OUTSIDE a read_*() call are intentionally left as
// quoted literals — DuckDB sees them as plain strings, which is harmless and
// avoids producing invalid SQL by injecting subquery text into a string
// context.
fn compile_substitute(sql: &str, subqueries: &HashMap<String, String>) -> String {
    // Pass 1: collapse read_csv_auto / read_xlsx calls.
    let mut acc = String::with_capacity(sql.len());
    let mut remaining = sql;
    loop {
        let csv = remaining.find("read_csv_auto(");
        let xlsx = remaining.find("read_xlsx(");
        let (idx, fn_len) = match (csv, xlsx) {
            (Some(c), Some(x)) if c < x => (c, "read_csv_auto(".len()),
            (Some(c), None) => (c, "read_csv_auto(".len()),
            (None, Some(x)) => (x, "read_xlsx(".len()),
            (Some(_), Some(x)) => (x, "read_xlsx(".len()),
            (None, None) => {
                acc.push_str(remaining);
                break;
            }
        };
        acc.push_str(&remaining[..idx]);
        let after = &remaining[idx + fn_len..];
        let close = after.find(')');
        match close {
            Some(end) => {
                let inner = &after[..end];
                let mut substituted = false;
                if let Some(p_start) = inner.find("{{input:") {
                    if let Some(p_end) = inner[p_start..].find("}}") {
                        let label = inner[p_start + 8..p_start + p_end].trim();
                        if let Some(sub) = subqueries.get(label) {
                            acc.push_str(sub);
                            substituted = true;
                        }
                    }
                } else if inner.contains("{{input_file}}") {
                    // Legacy single-input alias — pick the only registered
                    // subquery. Compile-check has already been gated on every
                    // declared input having validation columns, so subqueries
                    // is non-empty.
                    if let Some(sub) = subqueries.values().next() {
                        acc.push_str(sub);
                        substituted = true;
                    }
                }
                if !substituted {
                    // Couldn't resolve — leave the call untouched. DuckDB will
                    // report a useful parse error pointing at the placeholder.
                    acc.push_str(&remaining[idx..idx + fn_len + end + 1]);
                }
                remaining = &after[end + 1..];
            }
            None => {
                acc.push_str(&remaining[idx..]);
                break;
            }
        }
    }

    // Pass 2: only {{output:X}} placeholders, swapped for a sentinel string
    // so COPY ... TO '{{output:X}}' becomes COPY ... TO '__compile_check__'.
    // Input placeholders we didn't catch in pass 1 are left as string literals
    // — DuckDB treats them as plain strings and won't try to open a file
    // unless they're inside a read_*() we missed (which we report separately
    // as parse_failed).
    let mut out = String::with_capacity(acc.len());
    let mut rest = acc.as_str();
    while let Some(start) = rest.find("{{output:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find("}}") else {
            out.push_str(after);
            rest = "";
            break;
        };
        out.push_str("__compile_check_output");
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

// Run `EXPLAIN <stmt>` for every non-empty `;`-separated statement. Returns
// the index of the first failing statement plus DuckDB's error message.
fn explain_each(sql: &str) -> Result<(), (usize, String)> {
    let conn = Connection::open_in_memory()
        .map_err(|e| (0, format!("Cannot open DuckDB: {}", e)))?;
    for (i, stmt) in sql.split(';').enumerate() {
        let t = stmt.trim();
        if t.is_empty() {
            continue;
        }
        let wrapped = format!("EXPLAIN {}", t);
        if let Err(e) = conn.prepare(&wrapped) {
            return Err((i, e.to_string()));
        }
    }
    Ok(())
}

// ── scaffold_missing ─────────────────────────────────────────────────────────

pub fn scaffold_missing(
    files: &[ProfileFileEntry],
) -> Result<Vec<ProfileFileEntry>, crate::errors::AppError> {
    let yaml_file = files
        .iter()
        .find(|f| f.path == "structure.yaml")
        .ok_or_else(|| {
            crate::errors::AppError::ParseError(
                "structure.yaml is required to scaffold".to_string(),
            )
        })?;
    let structure: ProfileStructure = serde_yaml::from_str(&yaml_file.content)
        .map_err(|e| crate::errors::AppError::ParseError(format!("Invalid structure.yaml: {}", e)))?;

    let input_lookup: HashMap<&str, &InputDefinition> = structure
        .inputs
        .iter()
        .map(|i| (i.label.as_str(), i))
        .collect();

    // Clone existing files into a working set we can edit by path.
    let mut by_path: HashMap<String, String> = files
        .iter()
        .map(|f| (f.path.clone(), f.content.clone()))
        .collect();

    // ── Missing MD sections ──────────────────────────────────────────────────
    let md_current = by_path
        .get("instructions.md")
        .cloned()
        .unwrap_or_else(|| format!("# {}\n\n", structure.name));
    let md_anchors = parse_md_anchors(&md_current);
    let mut md_appendix = String::new();
    for step in &structure.steps {
        if md_anchors.contains_key(&step.label) {
            continue;
        }
        md_appendix.push_str(&format!(
            "\n<!-- label: {} -->\n## {}\n\n_TODO: describe this step._\n",
            step.label, step.label,
        ));
    }
    if !md_appendix.is_empty() {
        let mut next = md_current;
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push_str(&md_appendix);
        by_path.insert("instructions.md".to_string(), next);
    }

    // ── Missing SQL files ────────────────────────────────────────────────────
    for step in &structure.steps {
        if step.step_type != "sql_transform" {
            continue;
        }
        let units: Vec<(&[StepInputRef], &str, &[String], &[NoticeQuery])> =
            if let Some(ts) = &step.transforms {
                ts.iter()
                    .map(|t| {
                        (
                            t.input.as_deref().unwrap_or(&[]),
                            t.sql.as_str(),
                            t.output.as_deref().unwrap_or(&[]),
                            t.notices.as_deref().unwrap_or(&[]),
                        )
                    })
                    .collect()
            } else {
                vec![(
                    step.input.as_deref().unwrap_or(&[]),
                    step.sql.as_deref().unwrap_or(""),
                    step.output.as_deref().unwrap_or(&[]),
                    step.notices.as_deref().unwrap_or(&[]),
                )]
            };

        for (inputs, sql_name, outputs, notices) in units {
            if !sql_name.is_empty() {
                let bundle_path = format!("sql/{}", sql_name);
                if !by_path.contains_key(&bundle_path) {
                    by_path.insert(
                        bundle_path,
                        build_sql_stub(&step.label, inputs, outputs, &input_lookup),
                    );
                }
            }

            for notice in notices {
                if notice.sql.is_empty() {
                    continue;
                }
                let bundle_path = format!("sql/{}", notice.sql);
                if by_path.contains_key(&bundle_path) {
                    continue;
                }
                by_path.insert(
                    bundle_path,
                    build_notice_stub(&step.label, &notice.label, inputs, &input_lookup),
                );
            }
        }
    }

    // Re-emit in a stable order: structure.yaml, instructions.md, then sql/*.
    let mut out: Vec<ProfileFileEntry> = by_path
        .into_iter()
        .map(|(path, content)| ProfileFileEntry { path, content })
        .collect();
    out.sort_by(|a, b| sort_key(&a.path).cmp(&sort_key(&b.path)));
    Ok(out)
}

fn sort_key(path: &str) -> (u8, String) {
    if path == "structure.yaml" {
        (0, String::new())
    } else if path == "instructions.md" {
        (1, String::new())
    } else {
        (2, path.to_string())
    }
}

fn build_sql_stub(
    step_label: &str,
    inputs: &[StepInputRef],
    outputs: &[String],
    input_lookup: &HashMap<&str, &InputDefinition>,
) -> String {
    let primary_input = inputs.first().map(ref_label).unwrap_or_else(|| "Input".into());

    // Build the SELECT projection list from the primary input's declared
    // validation columns. If none are declared, fall back to `*`.
    let projection = input_lookup
        .get(primary_input.as_str())
        .and_then(|d| d.validation.as_deref())
        .filter(|v| !v.is_empty())
        .map(|cols| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| {
                    let prefix = if i == 0 { "    " } else { "    -- " };
                    format!("{}\"{}\"", prefix, c.label)
                })
                .collect::<Vec<_>>()
                .join(",\n")
        })
        .unwrap_or_else(|| "    *".to_string());

    let mut from_block = format!(
        "  FROM read_csv_auto('{{{{input:{}}}}}')",
        primary_input,
    );
    for r in inputs.iter().skip(1) {
        let lbl = ref_label(r);
        from_block.push_str(&format!(
            "\n  -- LEFT JOIN read_csv_auto('{{{{input:{}}}}}') USING (/* key */)",
            lbl,
        ));
    }

    let mut buf = String::new();
    buf.push_str(&format!("-- TODO: implement the {} transform.\n\n", step_label));

    if outputs.is_empty() {
        // No declared outputs — emit a bare SELECT shell. The validator will
        // already have flagged the missing outputs; this stub stays useful.
        buf.push_str(&format!(
            "SELECT\n{}\n{};\n",
            projection, from_block,
        ));
    } else {
        for (i, out_label) in outputs.iter().enumerate() {
            if i > 0 {
                buf.push('\n');
            }
            buf.push_str(&format!(
                "COPY (\n  SELECT\n{}\n{}\n) TO '{{{{output:{}}}}}' (HEADER, DELIMITER ',');\n",
                projection.replace("    ", "      "),
                from_block.replace("  ", "    "),
                out_label,
            ));
        }
    }

    buf
}

fn build_notice_stub(
    step_label: &str,
    notice_label: &str,
    inputs: &[StepInputRef],
    input_lookup: &HashMap<&str, &InputDefinition>,
) -> String {
    let primary_input = inputs.first().map(ref_label).unwrap_or_else(|| "Input".into());

    let projection = input_lookup
        .get(primary_input.as_str())
        .and_then(|d| d.validation.as_deref())
        .filter(|v| !v.is_empty())
        .map(|cols| {
            cols.iter()
                .enumerate()
                .map(|(i, c)| {
                    let prefix = if i == 0 { "    " } else { "    -- " };
                    format!("{}\"{}\"", prefix, c.label)
                })
                .collect::<Vec<_>>()
                .join(",\n")
        })
        .unwrap_or_else(|| "    *".to_string());

    let mut buf = String::new();
    buf.push_str(&format!(
        "-- TODO: implement the '{}' notice for the {} step.\n",
        notice_label, step_label,
    ));
    buf.push_str(
        "-- Each row returned becomes a flagged item shown to the user.\n-- Empty result = nothing to surface.\n\n",
    );
    buf.push_str(&format!(
        "SELECT\n{}\n  FROM read_csv_auto('{{{{input:{}}}}}')\n  WHERE FALSE; -- TODO: replace with the condition that flags a row\n",
        projection, primary_input,
    ));

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ProfileFileEntry;

    fn file(path: &str, content: &str) -> ProfileFileEntry {
        ProfileFileEntry { path: path.to_string(), content: content.to_string() }
    }

    // Two re_query steps and one consumer, in an order the caller controls.
    fn bundle(yaml: &str) -> Vec<ProfileFileEntry> {
        vec![
            file("structure.yaml", yaml),
            file("instructions.md", "# T\n\n<!-- label: Fetch -->\n## Fetch\n\n<!-- label: Build -->\n## Build\n"),
            file("sql/build.sql", "SELECT * FROM read_json_auto('{{query:RERecords}}');"),
        ]
    }

    const HEAD: &str = "id: t\nname: T\nversion: '1.0'\nmin_app_version: '0.1.0'\n\
                        inputs: []\noutputs:\n  - label: Out\n    type: csv\n";

    fn codes(report: &ValidationReport) -> Vec<&str> {
        report.issues.iter().map(|i| i.code.as_str()).collect()
    }

    #[test]
    fn query_input_in_order_is_accepted() {
        let yaml = format!(
            "{HEAD}steps:\n\
             \x20 - label: Fetch\n    type: re_query\n    ref: re.query.execute\n    query_output: RERecords\n\
             \x20 - label: Build\n    type: sql_transform\n    query_input: [RERecords]\n    sql: build.sql\n    output: [Out]\n"
        );
        let r = validate_profile(&bundle(&yaml));
        assert!(
            !codes(&r).contains(&"yaml.query_input.unresolved"),
            "issues: {:?}",
            r.issues.iter().map(|i| (&i.code, &i.message)).collect::<Vec<_>>()
        );
    }

    // The ordering bug this design can produce: reading a query declared later.
    #[test]
    fn forward_reference_is_rejected() {
        let yaml = format!(
            "{HEAD}steps:\n\
             \x20 - label: Build\n    type: sql_transform\n    query_input: [RERecords]\n    sql: build.sql\n    output: [Out]\n\
             \x20 - label: Fetch\n    type: re_query\n    ref: re.query.execute\n    query_output: RERecords\n"
        );
        let r = validate_profile(&bundle(&yaml));
        let msg = r
            .issues
            .iter()
            .find(|i| i.code == "yaml.query_input.unresolved")
            .map(|i| i.message.clone())
            .unwrap_or_default();
        assert!(msg.contains("comes later"), "issues: {:?}", codes(&r));
    }

    #[test]
    fn unknown_query_input_is_rejected() {
        let yaml = format!(
            "{HEAD}steps:\n\
             \x20 - label: Build\n    type: sql_transform\n    query_input: [Nope]\n    sql: build.sql\n    output: [Out]\n"
        );
        let r = validate_profile(&bundle(&yaml));
        assert!(codes(&r).contains(&"yaml.query_input.unresolved"));
        // The SQL's {{query:RERecords}} is unresolvable too.
        assert!(codes(&r).contains(&"sql.unknown_query"));
    }

    // ── sync_output / sync_input ──────────────────────────────────────────────
    // The same wiring rules, driven by the second UPSTREAM_FAMILIES entry.

    fn sync_bundle(yaml: &str) -> Vec<ProfileFileEntry> {
        vec![
            file("structure.yaml", yaml),
            file(
                "instructions.md",
                "# T\n\n<!-- label: Push -->\n## Push\n\n<!-- label: Build -->\n## Build\n",
            ),
            file("sql/push.sql", "SELECT 'x' AS long_description;"),
            file(
                "sql/build.sql",
                "SELECT * FROM read_json_auto('{{sync:NewCodes}}');",
            ),
        ]
    }

    const SYNC_PUSH: &str = "\x20 - label: Push\n    type: code_table_sync\n    \
                             code_table: Constituent Codes\n    operation: create\n    \
                             sql: push.sql\n    sync_output: NewCodes\n";
    const SYNC_BUILD: &str = "\x20 - label: Build\n    type: sql_transform\n    \
                              sync_input: [NewCodes]\n    sql: build.sql\n    output: [Out]\n";

    #[test]
    fn sync_input_in_order_is_accepted() {
        let yaml = format!("{HEAD}steps:\n{SYNC_PUSH}{SYNC_BUILD}");
        let r = validate_profile(&sync_bundle(&yaml));
        assert!(
            !codes(&r).contains(&"yaml.sync_input.unresolved"),
            "issues: {:?}",
            r.issues.iter().map(|i| (&i.code, &i.message)).collect::<Vec<_>>()
        );
        assert!(!codes(&r).contains(&"sql.unknown_sync"));
    }

    #[test]
    fn sync_input_before_its_producer_is_rejected() {
        // Build reads NewCodes but the sync producing it runs afterwards.
        let yaml = format!("{HEAD}steps:\n{SYNC_BUILD}{SYNC_PUSH}");
        let r = validate_profile(&sync_bundle(&yaml));
        let msg = &r
            .issues
            .iter()
            .find(|i| i.code == "yaml.sync_input.unresolved")
            .expect("expected an unresolved sync_input issue")
            .message;
        assert!(msg.contains("comes later"), "message was: {}", msg);
    }

    #[test]
    fn unknown_sync_input_is_rejected() {
        let yaml = format!(
            "{HEAD}steps:\n\
             \x20 - label: Build\n    type: sql_transform\n    sync_input: [Nope]\n    sql: build.sql\n    output: [Out]\n"
        );
        let r = validate_profile(&sync_bundle(&yaml));
        assert!(codes(&r).contains(&"yaml.sync_input.unresolved"));
        // build.sql's {{sync:NewCodes}} has no producer either.
        assert!(codes(&r).contains(&"sql.unknown_sync"));
    }

    #[test]
    fn duplicate_sync_output_is_rejected() {
        let yaml = format!("{HEAD}steps:\n{SYNC_PUSH}{SYNC_PUSH}{SYNC_BUILD}");
        let r = validate_profile(&sync_bundle(&yaml));
        assert!(codes(&r).contains(&"yaml.code_table_sync.duplicate_output"));
    }

    // re_query needs something runnable and a label to publish under.
    #[test]
    fn re_query_requires_call_and_output() {
        let yaml = format!(
            "{HEAD}steps:\n  - label: Fetch\n    type: re_query\n"
        );
        let r = validate_profile(&bundle(&yaml));
        assert!(codes(&r).contains(&"yaml.re_query.no_call"));
        assert!(codes(&r).contains(&"yaml.re_query.no_output"));
    }
}
