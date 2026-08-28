# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Windows desktop application built with **Tauri 2** (Rust backend + React frontend) that
transforms vendor-supplied CSV and Excel files into a target database's import format, and
renders live reports from the RE NXT (Blackbaud SKY) API. Both workflows live in
self-contained **profile bundles** (`.import` files, which are zip archives containing YAML,
SQL, Markdown, and optional JSON fixtures) — no recompiling required to add a new vendor or
report.

Two profile kinds share one bundle format and one loader:
- **import** (no `kind:` key) — `inputs` / `outputs` / `steps`; the Imports tab.
- **report** (`kind: report`) — `parameters` / `queries` / `transforms` /
  `visualizations` / `actions`; the Reports tab.

### Documentation map

| Doc | Covers |
|---|---|
| **`PROFILE_AUTHORING.md`** | **How to write a profile of either kind, start to finish. Point profile authors here first.** |
| `STEP_TYPES.md` | Per-step-type YAML + UI reference for import profiles |
| `REPORT_PROFILES.md` | Field-by-field contract + execution details for report profiles |
| `README.md` | Setup, dev commands, project layout |
| `import_tool_reference.md` | Architecture tour (Tauri, IPC, data flow, dependencies) |
| `API reference/` | SKY OpenAPI specs + Blackbaud's query-synchronize best practices |

---

## Commands

```bash
# Start the development environment (compiles Rust + starts Vite + opens window)
npm run tauri dev

# Build a distributable Windows binary
npm run tauri build

# Frontend only (no Tauri window — useful for pure UI work)
npm run dev

# Type-check without building
npx tsc --noEmit

# Verify + repack every profile bundle (run before a Rust build if a built-in changed)
./profiles/build.sh

# Force fixture ("mock") mode for every RE API call
RE_NXT_MOCK=1 npm run tauri dev
```

> **Rust recompiles are slow (5–30s).** Frontend changes hot-reload instantly.
> Stabilise Rust logic first, then iterate freely on the React side.

---

## Architecture — The Two-Process Model

```
Frontend (Renderer)          IPC Bridge              Backend (Main Process)
─────────────────────        ──────────────          ──────────────────────
React + TypeScript           invoke()                Rust binary
src/                         ← Promise →             src-tauri/src/
Handles: UI, display         returns Result<T,E>     Handles: files, DuckDB,
Cannot: touch filesystem                             profile parsing
```

**Rule of thumb:**
- Touches a file, database, or system resource → **Rust**
- Changes what the user sees → **React**
- Needs both → Frontend calls backend via `invoke()`, backend returns data, frontend displays it

---

## File Structure

```
tauri-import/
├── src/                          # FRONTEND — React app
│   ├── main.tsx                  # React entry point
│   ├── types.ts                  # Shared TypeScript types — mirrors Rust structs exactly
│   ├── App.tsx                   # Root component — all shared state + all invoke() calls
│   ├── lib/
│   │   ├── utils.ts              # cn() helper for Tailwind class composition
│   │   ├── profile-utils.ts      # Profile summary/list helpers
│   │   └── re-calls.ts           # TS mirror of the RE call registry (drives UI labels/params)
│   └── components/
│       ├── Titlebar.tsx          # Top window chrome + workspace tabs
│       ├── SettingsPanel.tsx     # Settings shell
│       ├── PanelTransition.tsx   # Workspace switch animation
│       ├── imports/              # IMPORTS workspace
│       │   ├── ImportsPage.tsx
│       │   ├── Sidebar.tsx       # Profile picker + step progress list
│       │   ├── MainPanel.tsx     # One section per step; `switch (step.type)` dispatch
│       │   └── steps/
│       │       ├── StepSelectFiles.tsx   # file_input
│       │       ├── StepGenerateFile.tsx  # sql_transform
│       │       ├── StepQuery.tsx         # re_query
│       │       ├── StepCodeTableSync.tsx # code_table_sync
│       │       └── StepImport.tsx        # manual_instruction
│       ├── reports/              # REPORTS workspace
│       │   ├── ReportsPage.tsx   # Refresh + stale handling + viz layout
│       │   ├── ReportInputs.tsx  # `parameters` → form controls
│       │   └── viz/              # VIZ_REGISTRY; only TableViz is implemented
│       ├── data-request/         # DATA REQUESTS workspace (library + status)
│       ├── settings/
│       │   ├── general/          # RE NXT connection
│       │   └── imports/          # In-app profile editor (tree, editor, issues, scaffold)
│       ├── shared/               # CodeMirrorEditor, NoticeBlock, Panel
│       └── ui/                   # Shadcn primitives (button, popover, command)
│
├── src-tauri/                    # BACKEND — Rust binary
│   ├── Cargo.toml                # Rust dependencies
│   └── src/
│       ├── main.rs               # Entry point — registers all commands
│       ├── commands.rs           # #[tauri::command] functions (thin API layer)
│       ├── profile.rs            # Bundle load/save/duplicate/create; all YAML structs
│       ├── validate.rs           # Profile linting (issue codes) + scaffold_missing
│       ├── db.rs                 # DuckDB validation, SQL transforms, ResultSets
│       ├── re_calls.rs           # The one place a SKY API call is executed (Query + Code Table)
│       ├── code_tables.rs        # Pulls RE code tables into SQL; runs code_table_sync writes
│       │                         #   and publishes their outcomes as {{sync:Label}}
│       ├── query_step.rs         # Runs an re_query step: SQL params → RE query → JSON for later SQL
│       ├── report.rs             # Report pipeline: params → queries → transforms → result sets
│       ├── sky_auth.rs           # RE NXT OAuth connect/status/token
│       └── errors.rs             # Shared AppError enum
│
├── profiles/                     # PROFILE BUNDLES — not compiled in, ship alongside exe
│   ├── src/<name>/               # Source folder per profile — structure.yaml, instructions.md,
│   │                             #   sql/, optional fixtures/, assets/, test-files/
│   ├── build.sh                  # Verifies (both kinds) then repacks each src/<name>/ into <name>.import
│   └── <name>.import             # zip archive consumed by the running app
│
└── API reference/                # SKY OpenAPI specs + query-synchronize best practices
```

> `test-files/` is excluded when `build.sh` packs a bundle; `fixtures/` is not —
> mock mode reads fixtures out of the shipped zip.

---

## The Backend Commands

Every command must be registered in `main.rs` inside `generate_handler![]` or `invoke()` fails silently.

**Profiles and imports** (`commands.rs`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `list_profiles` | `App.tsx` on mount | _(none from frontend; `AppHandle` injected by Tauri)_ — returns embedded built-ins + `.import` files in `app_data_dir()/profiles/` | `ProfileSummary[]` |
| `load_profile` | `App.tsx` on profile select | `zipPath` | `LoadedProfile` |
| `validate_file` | `App.tsx` on validate click | `filePath`, `inputLabel`, `zipPath` | `ValidationResult` |
| `run_profile` | `App.tsx` on generate click | `filePaths` (map of input label → file path), `queryPaths` (query label → result JSON), `syncPaths` (sync label → outcome JSON), `sqlFile`, `zipPath`, `outputLabels` | `TransformResult` |
| `run_re_query` | `App.tsx` on an `re_query` step | `filePaths`, `stepLabel`, `zipPath` | `QueryStepResult` |
| `run_code_table_sync` | `App.tsx` on a `code_table_sync` step | `filePaths`, `stepLabel`, `zipPath` | `SyncResult` |
| `save_output` | `App.tsx` on download click | `srcPath`, `destPath` | `void` |

**Reports** (`commands.rs` → `report.rs`; both `async`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `run_report` | `ReportsPage` Refresh | `zipPath`, `paramValues` | `ReportRunResult` (`data`, `queries`, `generated_at`, `mode`) |
| `run_report_action` | `ReportsPage` action button | `zipPath`, `actionId`, `paramValues` | `ActionResult` |

**In-app profile editor** (`commands.rs` → `profile.rs` / `validate.rs`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `new_profile` | Settings → Imports, "New profile" | _(AppHandle only)_ | `ProfileMutation` |
| `duplicate_profile` | Settings → Imports, "Duplicate" or opening a built-in | `sourceZipPath` | `ProfileMutation` |
| `save_profile` | Settings → Imports, Save | `zipPath`, `files` | `ProfileMutation` |
| `delete_profile` | Settings → Imports, Delete (refuses built-ins) | `zipPath` | `void` |
| `validate_profile` | Settings → Imports, Validate | `files` | `ValidationReport` |
| `scaffold_missing` | Settings → Imports, "Scaffold missing files" | `files` | `ProfileFileEntry[]` |

**RE NXT connection** (`sky_auth.rs`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `connect_re_nxt` | Settings → General (async, opens a browser for OAuth) | `clientId`, `clientSecret`, `subscriptionKey` | `ConnectionStatus` |
| `re_nxt_status` | `App.tsx` on mount (no network I/O) | _(AppHandle only)_ | `ConnectionStatus` |
| `disconnect_re_nxt` | Settings → General | _(AppHandle only)_ | `void` |
| `re_nxt_access_token` | Callers needing a live token (async) | _(AppHandle only)_ | `String` |

`zipPath` for `validate_file`, `run_profile`, `run_re_query`,
`run_code_table_sync`, `run_report`, and `run_report_action` is actually the
extracted temp dir from `loadedProfile.temp_dir`, not the original `.import` zip.
Naming kept for backwards compatibility — the backend re-reads `structure.yaml`,
SQL, and fixtures from that directory. `save_profile` / `delete_profile` /
`duplicate_profile` take the **real** `.import` path instead, since they write
the bundle.

**Live vs mock.** Every RE-touching command picks a `Transport`: `Live` when a
connection exists (`sky_auth::has_connection`) and `RE_NXT_MOCK` is unset,
otherwise `Mock`, which reads the bundle's `fixtures/`. The chosen mode is
returned to the frontend and shown as a badge.

### Profile sources

- **Built-ins**: embedded at compile-time via `include_bytes!` in `profile.rs::BUILTIN_PROFILES`. The `.import` files must exist when Rust builds — run `profiles/build.sh` first if you've changed a built-in's source.
- **User profiles**: `.import` files in `app_data_dir()/profiles/` — resolved via `AppHandle::path()`. macOS: `~/Library/Application Support/com.navin.tauri-import/profiles/`. Auto-created on first `list_profiles`.
- **`zip_path` shapes**: user profiles use a real fs path; built-ins use the sentinel `builtin://<filename>`. `load_profile` strips that prefix and extracts from in-memory bytes.
- **Frontend selection keys on `zip_path`, not `id`** — a built-in and a user profile can share an `id`; only `zip_path` is unique.

---

## Profile Bundle Format

> Writing or editing a profile (rather than the app)? Use
> **`PROFILE_AUTHORING.md`** — it covers both kinds end to end. What follows is
> the short orientation for people changing app code.

Each profile is a **`.import` file** (a renamed zip) containing:

- **`structure.yaml`** — metadata (id, name, version) plus either the import sections (`inputs`, `outputs`, `steps`) or, with `kind: report`, the report sections (`parameters`, `queries`, `transforms`, `visualizations`, `actions`). `code_tables:` is valid in both.
- **`instructions.md`** — step-by-step markdown, split into sections by `<!-- label: StepLabel -->` HTML comments; content before the first anchor is stored as `_header`
- **`sql/`** — folder of `.sql` files; each `sql_transform` step names one file via `step.sql`
- **`fixtures/`** — canned RE responses for mock mode: `<output>.json` (report queries), `queries/<query_output>.json` (`re_query` steps), `codetables/<output>.json` (code tables)
- **`assets/`** — images referenced from `instructions.md`

`structure.yaml` schema (import kind):
```yaml
id: vendor_a
name: "Vendor A Import"
version: "1.0"
min_app_version: "0.1.0"
inputs:
  - label: Classification
    type: csv
    required: true
    validation:
      - label: "Item #"
        required: true
        type: number
        digits: 6
      - label: Category
        required: true
        type: string
        value: ["Alpha", "Beta", "Gamma"]   # allowed values
outputs:
  - label: "Import File"
    type: csv
steps:
  - label: Upload File
    type: file_input
    input:
      - label: Classification
        validate: true
  - label: Transform
    type: sql_transform
    input: ["Classification"]
    sql: primary_transform.sql
    output: ["Import File"]
```

Step types supported: `file_input`, `sql_transform`, `re_query`,
`code_table_sync`, `manual_instruction`.
Validation is not its own step type — it's a per-row checkbox inside a
`file_input` step.

`sql_transform` steps can declare multiple transforms via a `transforms:` array
(each entry has its own `input`, `sql`, `output`, optional `notices`), or use
the step-level `input`/`sql`/`output` shortcut for a single transform.

### SQL placeholders

DuckDB reads input files directly via `read_csv_auto(...)` or `read_xlsx(...)`.
Three placeholder forms are substituted at runtime:

- `{{input:Label}}` — resolves to the path of the input with that label.
  Required when the transform declares multiple inputs.
- `{{input_file}}` — legacy single-input alias. Only valid when the transform
  has exactly one input; otherwise the run errors out.
- `{{output:Label}}` — resolves to a temp-dir path for the declared output.
  When present, the SQL author writes their own `COPY (...) TO '{{output:X}}'`
  statements (multi-output mode). When absent, the SQL is treated as a bare
  `SELECT` and wrapped in a `COPY` to the single declared output.

- `{{codetable:Label}}` — resolves to a JSON file holding one RE code table's
  entries, pulled before the SQL runs. Declared in the top-level `code_tables:`
  section (valid in both profile kinds); read with `read_json_auto`. See
  STEP_TYPES.md → *Code tables*.
- `{{query:Label}}` — resolves to the JSON an earlier `re_query` step returned.
  The consuming transform must declare the label in `query_input`. Read with
  `read_json_auto`. See STEP_TYPES.md → *Step type: `re_query`*.
- `{{sync:Label}}` — resolves to the outcome rows an earlier `code_table_sync`
  step published via `sync_output` (one row per attempted write, carrying the id
  RE assigned). The consuming transform must declare the label in `sync_input`.
  Read with `read_json(..., columns={...})` — the result is empty when nothing
  needed syncing. See STEP_TYPES.md → *Step type: `code_table_sync`*.

Column names with spaces, `#`, `/` etc. must be double-quoted in SQL: `"Item #"`.

---

## Frontend State Architecture

`App.tsx` owns all shared state (`files`, `generations`, `selectedProfile`) and
is the **only** place that calls `invoke()`. Handlers (`handleFileSelect`,
`handleValidate`, `handleGenerate`, `handleDownload`, `handleSelectProfile`)
live in `App.tsx` and are passed down as props.

Render hierarchy:
- `App.tsx` → `Titlebar` + the active workspace (`imports/ImportsPage`,
  `reports/ReportsPage`, `data-request/DataRequestsPage`) + `SettingsPanel`
- `imports/MainPanel.tsx` iterates `loadedProfile.structure.steps` and
  dispatches each to the matching step component:
  - `imports/steps/StepSelectFiles.tsx` for `file_input` — one row per declared
    input, each with Upload + filename pill + Validate button. Inline error
    table when validation fails.
  - `imports/steps/StepGenerateFile.tsx` for `sql_transform` — pipeline diagram
    (inputs → DB → outputs) + Generate button + progress bar + per-output
    Download buttons. Renders one pipeline per transform when the step has a
    `transforms:` array.
  - `imports/steps/StepQuery.tsx` for `re_query` — pipeline diagram + Run Query
    + indeterminate progress + the resulting `{{query:Label}}` hint.
  - `imports/steps/StepCodeTableSync.tsx` for `code_table_sync` — live/mock
    badge, operation-labelled button, success/partial callout + failures table.
  - `imports/steps/StepImport.tsx` for `manual_instruction` — renders the
    markdown body with image assets resolved against `loadedProfile.temp_dir`.
- `reports/ReportsPage.tsx` renders `ReportInputs` (from `parameters`) plus one
  component per `visualizations` entry, looked up in
  `reports/viz/index.tsx`'s `VIZ_REGISTRY` — the report-side mirror of
  `MainPanel`'s step dispatch. Only `TableViz` is implemented; the chart types
  render placeholders.
- `settings/imports/ImportTab.tsx` owns the in-app profile editor and is the
  one exception to the "only `App.tsx` calls `invoke()`" rule — it runs the
  editor commands itself and keeps its view children pure.

The `loadedProfile.temp_dir` is threaded through `MainPanel` to each step
component and passed back to `validate_file`, `run_profile`, `run_re_query`,
`run_code_table_sync`, and `run_report` so Rust can re-read validation rules,
SQL, and fixtures.

---

## Types

`src/types.ts` mirrors the Rust structs in `profile.rs` exactly. If you change a struct in Rust, update the matching type in `types.ts`. Import all types from `types.ts` — never inline them.

---

## Error Handling

- All Rust errors flow through `AppError` in `errors.rs`
- Commands convert `AppError` to `String` for the frontend via `.map_err(|e| e.to_string())`
- Every `invoke()` call in TypeScript must be wrapped in `try/catch` — rejections carry the Rust error string

---

## Style Rules

### Rust
- All functions that can fail return `Result<T, AppError>`
- On Windows: replace `\` with `/` in file paths before injecting into SQL strings (DuckDB accepts both)

### TypeScript / React
- Always `await` every `invoke()` call — forgetting causes silent failures
- State shared across components lives in `App.tsx`
- File paths come from Tauri's file dialog APIs — do not manipulate them in JS

### General
- Never refactor code unless explicitly asked
- Make minimal changes — only touch the files relevant to the task

---

## Common Pitfalls

- **Forgot to register a command?** → Check `generate_handler![]` in `main.rs`
- **`invoke()` silently failing?** → Confirm the command is registered and you `await`-ed the call
- **DuckDB column error?** → Wrap column names with special characters in double quotes
- **Windows path backslash in SQL?** → Replace `\` with `/` in `db.rs` before string substitution
- **Excel header not on row 1?** → Add `OFFSET 1` or use a CTE in the profile SQL
- **State read too early?** → Use the value returned by the setter callback, not the stale state variable
- **`validate_file` / `run_profile` `zipPath` arg is the extracted temp dir, not the .import zip** → naming is misleading; the backend reads `structure.yaml` and SQL straight from that directory
- **`{{input_file}}` errors in a multi-input transform** → use `{{input:Label}}` placeholders to disambiguate, one per declared input
- **`{{codetable:X}}` errors?** → the label must match a `code_tables:` entry's
  `output`, not the RE table's name. Both `validate.rs` and `build.sh` check this
- **Code table step does nothing?** → check the live/mock badge. Without an RE
  connection (or with `RE_NXT_MOCK=1`) writes are stubbed, not sent
- **`{{query:X}}` unresolved at runtime?** → the consuming transform must declare
  `X` in `query_input`, and the producing `re_query` step must appear *earlier*
  in `steps:` and have been run. Both verifiers catch the ordering case
- **`{{sync:X}}` unresolved at runtime?** → same rule with `sync_input` and a
  `code_table_sync` step declaring `sync_output: X`. The step must have been run
  even when it had nothing to push — a zero-row run still publishes `[]`
- **Binder error on a `{{sync:X}}` column?** → the sync attempted zero rows, so
  `read_json_auto` has no schema to infer. Use
  `read_json('{{sync:X}}', columns={...})` naming the columns you join on
- **`{{rows:col}}` sent as a string instead of an array?** → it only becomes an
  array when the YAML value is *exactly* that placeholder, e.g.
  `filter_values: "{{rows:record_id}}"`, not embedded in a longer string
- **Two profiles with the same name in the sidebar?** → built-in and user profile share an `id`; this is expected. Select on `zip_path`, never on `id`
- **Edited a built-in profile's source and nothing changed?** → run
  `./profiles/build.sh` and restart `npm run tauri dev`; built-ins are embedded
  at compile time via `include_bytes!`
- **Report chart renders as an empty dashed box?** → only `table` is implemented
  in `VIZ_REGISTRY`; `bar`/`line`/`pie`/`kpi` are placeholder stubs
- **Report profile passes in-app Validate but fails `build.sh`?** → `validate.rs`
  implements the import rules; the report sections are only checked by `build.sh`

---

## What NOT to Do

- Do not modify `build.rs` — it is the Tauri build script
- Do not put file I/O or DuckDB logic in React components
- Do not put UI state management in Rust commands
- Do not manipulate Windows file paths in JavaScript
- Do not add a Cargo dependency without checking if it's already available (slow recompile)
