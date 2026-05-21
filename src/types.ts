// types.ts
//
// TypeScript types that mirror the Rust structs in profile.rs exactly.
// If you change a struct in Rust, update the matching type here.
// These are used throughout the frontend — import from here, not inline.

// ── Profile listing ───────────────────────────────────────────────────────────
// Returned by list_profiles — lightweight, just enough for the dropdown

export type ProfileSource = "builtin" | "user";

export type ProfileSummary = {
  id: string;
  name: string;
  version: string;
  zip_path: string;        // user: full fs path. builtin: "builtin://<filename>" sentinel.
  source: ProfileSource;
};

// ── Structure.yaml types ──────────────────────────────────────────────────────
// Mirror the YAML schema exactly

export type ColumnValidation = {
  label: string;
  required: boolean;
  type: string;            // "string" | "number"
  digits?: number;         // only for number columns
  value?: string[];        // allowed values if restricted e.g. ["Alpha", "Beta", "Gamma"]
};

export type InputDefinition = {
  label: string;
  type: string;            // "csv" | "xlsx"
  required: boolean;
  validation?: ColumnValidation[];
};

export type OutputDefinition = {
  label: string;
  type: string;            // "csv"
};

// Steps can reference inputs in two ways:
// Simple:   "Classification"
// Detailed: { label: "Classification", validate: true }
export type StepInputRef =
  | string
  | { label: string; validate?: boolean };

// A notice query attached to a sql_transform. Runs after the main transform
// succeeds; returned rows are surfaced as informational items (NOT errors)
// that the user may need to address externally before moving on.
// The SQL is expected to return zero rows in the nominal case and one row
// per item-needing-attention otherwise. Column headers from the result set
// drive the displayed table columns.
export type NoticeQuery = {
  label: string;            // shown as the notice heading
  sql: string;              // filename inside the bundle's sql/ folder
  description?: string;     // optional sub-heading prose
};

// One transform unit inside a sql_transform step. A step can contain one
// (use the step-level input/sql/output fields) or many (use the transforms
// array). When `transforms` is present, the step-level fields are ignored.
export type SqlTransform = {
  input?: StepInputRef[];
  sql: string;
  output?: string[];
  notices?: NoticeQuery[];
};

export type Step = {
  label: string;
  type: string;            // "file_input" | "sql_transform" | "manual_instruction"
  input?: StepInputRef[];  // file_input: one upload row per entry. sql_transform: single-transform shortcut.
  sql?: string;            // sql_transform single-transform shortcut
  output?: string[];       // sql_transform single-transform shortcut
  notices?: NoticeQuery[]; // sql_transform single-transform shortcut
  transforms?: SqlTransform[]; // sql_transform multi-transform form
};

// One row in a file_input step's validation-errors table. Most fields are
// optional because the backend currently aggregates per-column (e.g. "3 nulls
// in column X") rather than enumerating offending rows.
export type ValidationError = {
  row?: number;
  column?: string;
  value?: string;
  message: string;
};

// One row in a sql_transform's SQL/DuckDB error table.
export type SqlError = {
  line?: number;
  errorType: string;   // "Parser" | "Binder" | "Runtime" | etc.
  message: string;
};

// A populated notice returned from the backend after running a NoticeQuery.
// `columns` are the header names from the SQL result set, in order.
// `rows` are the data rows (each cell stringified for display).
export type Notice = {
  label: string;
  description?: string;
  columns: string[];
  rows: string[][];
};

// One file emitted by a sql_transform. A transform can produce many of these
// when its SQL uses {{output:Label}} placeholders, one per declared output.
export type OutputFile = {
  label: string;
  path: string;
  row_count: number;
};

// Result returned by run_profile — mirrors db::TransformResult in Rust.
export type TransformResult = {
  outputs: OutputFile[];
  notices: Notice[];
};

export type ProfileStructure = {
  id: string;
  name: string;
  version: string;
  min_app_version: string;
  inputs: InputDefinition[];
  outputs: OutputDefinition[];
  steps: Step[];
};

// ── Loaded profile ────────────────────────────────────────────────────────────
// Returned by load_profile — full parse including instructions and SQL

export type LoadedProfile = {
  structure: ProfileStructure;
  instructions: Record<string, string>; // step label → markdown content
  sql_files: Record<string, string>;    // filename → SQL content
  temp_dir: string;
  files: ProfileFileEntry[];             // raw editable text files (structure.yaml, instructions.md, sql/*.sql)
};

// ── Profile editor ───────────────────────────────────────────────────────────
// One editable file inside a profile bundle. Path is relative to bundle root
// with forward-slash separators (e.g. "structure.yaml", "sql/primary.sql").

export type ProfileFileEntry = {
  path: string;
  content: string;
};

// Returned by save_profile / new_profile / duplicate_profile — gives the
// frontend both the refreshed sidebar summary and the new file contents
// in a single round-trip.
export type ProfileMutation = {
  summary: ProfileSummary;
  loaded: LoadedProfile;
};

// ── Profile validator ────────────────────────────────────────────────────────
// Mirrors validate.rs in the backend.

export type Severity = "error" | "warning" | "info";

// Tagged union — `kind` is the discriminator. Some variants carry a label
// (resolved to a line number on the frontend via the editor's anchor map);
// others carry a raw line number from a parser; others carry a file path
// for file-level issues.
export type IssueLocation =
  | { kind: "yaml_step"; label: string }
  | { kind: "yaml_input"; label: string }
  | { kind: "yaml_output"; label: string }
  | { kind: "yaml_line"; line: number }
  | { kind: "md_anchor"; label: string }
  | { kind: "md_line"; line: number }
  | { kind: "sql"; path: string; line?: number | null }
  | { kind: "file"; path: string };

export type ValidationIssue = {
  severity: Severity;
  code: string;          // stable identifier (e.g. "yaml.duplicate_step_label")
  message: string;
  location?: IssueLocation | null;
  fixable: boolean;      // true if `scaffold_missing` would address it
};

export type ValidationReport = {
  issues: ValidationIssue[];
  error_count: number;
  warning_count: number;
  info_count: number;
  fixable_count: number;
};

