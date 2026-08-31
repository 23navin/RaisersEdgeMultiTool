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
  kind?: string | null;    // undefined/"import" => import workflow. "report" => report.
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
  query_input?: string[];   // query outputs this transform reads
  sync_input?: string[];    // code_table_sync outcomes this transform reads
};

// The display spec on a `visualization` step. Mirrors a report profile's
// `visualizations:` entry minus id/data — the step's label is its id and the
// rows come from the step's own SQL. Both kinds render through VIZ_REGISTRY.
export type StepVisualization = {
  type: string;            // "table" | "bar" | "line" | "pie" | "kpi"
  title?: string;
  config?: unknown;        // viz-specific (e.g. table column definitions)
};

export type Step = {
  label: string;
  // "file_input" | "sql_transform" | "re_query" | "code_table_sync"
  // | "visualization" | "manual_instruction"
  type: string;
  input?: StepInputRef[];  // file_input: one upload row per entry. sql_transform: single-transform shortcut.
  sql?: string;            // sql_transform single-transform shortcut; code_table_sync: rows to push
  output?: string[];       // sql_transform single-transform shortcut
  notices?: NoticeQuery[]; // sql_transform single-transform shortcut
  transforms?: SqlTransform[]; // sql_transform multi-transform form
  // code_table_sync fields
  code_table?: string;     // code table name...
  code_table_id?: string;  // ...or its id, skipping the name lookup
  operation?: string;      // "create" | "update" | "delete"
  // Names this step's outcome rows -> {{sync:<label>}} downstream. Optional:
  // a sync nothing reads needs no label.
  sync_output?: string;
  // re_query fields
  ref?: string;            // central registry call, e.g. re.query.execute
  template?: unknown;      // inline ExecuteQueryDefinition
  bind?: Record<string, unknown>;
  params_sql?: string;     // SQL whose rows fill {{rows:}} / {{value:}}
  query_output?: string;   // names the result -> {{query:<label>}} downstream
  // sql_transform: query outputs this transform reads. Distinct from `output`,
  // which means "a declared file with a Download button".
  query_input?: string[];
  // sql_transform: code_table_sync outcomes this transform reads as
  // {{sync:<label>}}.
  sync_input?: string[];
  // visualization: how to draw the rows the step's `sql` returns. Omit `sql`
  // and the step's single query_input / sync_input is shown as-is.
  visualization?: StepVisualization;
};

// A code table pulled from RE before SQL runs, exposed to it as
// {{codetable:<output>}}. Mirrors profile::CodeTableRef. Both profile kinds.
export type CodeTableRef = {
  id: string;
  name?: string | null;
  code_table_id?: string | null;
  include_inactive?: boolean | null;
  output: string;
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

// Returned by validate_file. Mirrors db::ValidationResult.
export type ValidationResult = {
  ok: boolean;
  errors: ValidationError[];
  notices: Notice[];
};

// One file emitted by a sql_transform. A transform can produce many of these
// when its SQL uses {{output:Label}} placeholders, one per declared output.
export type OutputFile = {
  label: string;
  artifact_id: string;     // opaque id resolved server-side, inside the session
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
  kind?: string;           // undefined/"import" => import workflow. "report" => report sections.
  // Import sections — always present for import profiles, empty arrays for reports.
  inputs: InputDefinition[];
  outputs: OutputDefinition[];
  steps: Step[];
  // Shared section — RE code tables available to SQL in either profile kind.
  code_tables: CodeTableRef[];
  // Report sections — populated only when kind === "report". See REPORT_PROFILES.md.
  parameters: Parameter[];
  queries: QueryRef[];
  transforms: ReportTransform[];
  visualizations: Visualization[];
  actions: Action[];
};

// ── Report profile types ──────────────────────────────────────────────────────
// Mirror the report-specific structs in profile.rs. Free-form fields
// (default/config/template/bind values) are arbitrary JSON. Deserialize-only on
// the backend for now — nothing executes these yet.

// A UI input control rendered in the Inputs panel.
export type Parameter = {
  id: string;
  label: string;
  type: string;            // "date" | "date_range" | "select" | "text" | "number"
  required?: boolean;
  options?: string[];      // allowed values for "select"
  default?: unknown;       // shape varies by type (e.g. { preset: "last_30_days" })
};

// A reference to an RE API call (hybrid library). `ref` names a central registry
// entry; `template` inlines a bundle-local definition. `bind` maps params/
// literals into the call; `output` names the JSON result transforms read via
// {{query:Label}}.
export type QueryRef = {
  id: string;
  ref?: string;
  template?: unknown;
  bind?: Record<string, unknown>;
  output: string;
};

// SQL over query outputs. `input` lists query output labels; `output` is the
// in-memory result-set label a visualization binds to.
export type ReportTransform = {
  id: string;
  input?: string[];
  sql: string;            // filename inside the bundle's sql/ folder
  output: string;
};

// A visualization bound to a transform output, rendered by the shared viz
// library keyed on `type`.
export type Visualization = {
  id: string;
  type: string;           // "table" | "bar" | "line" | "pie" | "kpi"
  title?: string;
  data: string;           // transform output label feeding it
  config?: unknown;       // viz-specific config (e.g. column definitions)
};

// An on-demand write-back. Resolves through the API library like QueryRef;
// `input` names the result set whose rows feed the call.
export type Action = {
  id: string;
  label: string;
  ref?: string;
  template?: unknown;
  input?: string;
  bind?: Record<string, unknown>;
};

// A processed result set produced by a report transform and consumed by a
// visualization. Same shape as Notice (columns + stringified rows).
export type ResultSet = {
  columns: string[];
  rows: string[][];
};

// One query's resolved request + outcome — surfaced so the UI can show/debug
// the param→request merge. Mirrors report::QueryDebug.
export type QueryDebug = {
  id: string;
  call_ref?: string | null;
  resolved_request: unknown;   // the request after {{param:...}} substitution
  row_count: number;
};

// Returned by run_report. `data` is keyed by transform output label — exactly
// what a visualization's `data` field binds to. Mirrors report::ReportRunResult.
export type ReportRunResult = {
  data: Record<string, ResultSet>;
  queries: QueryDebug[];
  generated_at: string;
  mode: string;            // "live" (real SKY API) | "mock" (fixtures)
};

// Returned by run_re_query. Mirrors query_step::QueryStepResult.
export type QueryStepResult = {
  query_output: string;    // the label later SQL uses as {{query:<label>}}
  artifact_id: string;     // opaque id of the JSON the rows were written to
  row_count: number;
  mode: string;            // "live" (real SKY API) | "mock"
  resolved_request: unknown; // request after {{rows:}}/{{value:}} substitution
};

// Returned by run_code_table_sync. Mirrors code_tables::SyncResult.
export type SyncFailure = {
  row: number;
  identifier: string;      // long_description or entry id — a human-readable pointer
  error: string;
};

export type SyncResult = {
  ok: boolean;             // false when any row failed
  operation: string;       // "create" | "update" | "delete"
  code_table: string;      // how the table was addressed, for display
  attempted: number;
  succeeded: number;
  failures: SyncFailure[]; // one per failed row — the run continues past failures
  message: string;
  mode: string;            // "live" (real SKY API) | "mock"
  // Set when the step declares sync_output: the label later SQL reads as
  // {{sync:<label>}}, and the opaque id of the JSON the outcome rows were
  // written to.
  sync_output?: string | null;
  artifact_id?: string | null;
};

// Returned by run_report_action. Mirrors report::ActionResult.
export type ActionResult = {
  ok: boolean;
  message: string;
};

// ── Loaded profile ────────────────────────────────────────────────────────────
// Returned by load_profile — full parse including instructions and SQL

export type LoadedProfile = {
  structure: ProfileStructure;
  instructions: Record<string, string>; // step label → markdown content
  sql_files: Record<string, string>;    // filename → SQL content
  // Opaque session handle — echoed back on every later call in place of a
  // filesystem path. Minted by load_profile.
  session_id: string;
  // Base for relative asset references in instructions.md (images). Desktop:
  // the extracted profile dir; web (later): a URL prefix.
  asset_base: string;
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

// ── Raiser's Edge NXT connection ───────────────────────────────────────────────
// Mirrors sky_auth::ConnectionStatus. Tokens/secret are never sent to the
// frontend — only what the UI needs to show connection state. Returned by
// connect_re_nxt and re_nxt_status.

export type ReNxtConnectionStatus = {
  connected: boolean;
  environment_id?: string | null;
  environment_name?: string | null;
  expires_at?: number | null;   // unix seconds until the access token expires
  // True when RE_NXT_MOCK pins every RE call to bundle fixtures. Independent
  // of `connected` — a stored connection can exist while mock mode overrides it.
  mock_forced?: boolean;
};

