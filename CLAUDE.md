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
| **`SERVER.md`** | **Deploying, configuring, and operating the web server (config, RE connection, HTTP API, security posture)** |
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

## Architecture — Engine + Shells

The Rust engine lives in `crates/core` (no Tauri, no HTTP) and is wrapped by
thin shells. The desktop shell (`src-tauri`) exposes each `core::api` operation
as a `#[tauri::command]`; a web shell (`crates/server`, Axum) exposes the same
operations as `POST /api/<command>`. One React app serves both — it calls the
backend through `src/lib/api.ts`, which picks `invoke()` inside the Tauri
webview and `fetch()` in a plain browser.

```
Frontend (React, src/)      src/lib/api.ts          Backends
─────────────────────       ───────────────         ─────────────────────────
components                  isTauri ? invoke()      src-tauri  → core::api
  ↓ typed fns                       : fetch()       crates/server → core::api
Handles: UI, display        same JSON both ways     crates/core: files, DuckDB,
Cannot: touch filesystem                              profile parsing, SKY calls
```

**Rule of thumb:**
- Touches a file, database, or system resource → **`crates/core`** (plain fn in `api.rs`)
- Tauri- or HTTP-specific glue (dialogs, OAuth loopback, routes) → **the shell**
- Changes what the user sees → **React**
- Needs both → component calls a typed fn in `src/lib/api.ts`, backend returns data

**RE NXT connection (both shells).** The model, the on-disk format
(`re_nxt_connection.json`), and token exchange/refresh live in
`core/src/creds.rs`, so a connection is portable between desktop and server.
Only code acquisition differs: desktop binds a loopback listener and opens the
system browser (`sky_auth.rs`); the server hands the browser an authorization
URL and receives the redirect at `GET /api/oauth/callback`. Both then call
`creds::exchange_code`. The server's connection is **server-wide** — one person
connects through Settings → General and everyone shares it; per-user
connections need the identity model that doesn't exist yet.

**Session model (both shells).** `load_profile` mints a session under
`<workspaces_root>/<session_id>/` (`core/src/workspace.rs`), extracts the
bundle into it, and returns `session_id` on the `LoadedProfile`. Every later
call echoes `sessionId` back; files produced by steps cross the wire as
**opaque artifact ids** (session-relative paths, containment-checked on
resolve) — the client never sees a server path. Stale sessions are reaped
after 24h on the next `load_profile`.

---

## File Structure

```
tauri-import/
├── src/                          # FRONTEND — React app
│   ├── main.tsx                  # React entry point
│   ├── types.ts                  # Shared TypeScript types — mirrors Rust structs exactly
│   ├── App.tsx                   # Root component — all shared state + backend calls
│   ├── lib/
│   │   ├── api.ts                # THE transport module — invoke() vs fetch(), file pick/save, asset URLs
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
│       │       ├── StepUserInput.tsx     # user_input
│       │       ├── StepVisualize.tsx     # visualization
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
├── Cargo.toml                    # WORKSPACE root — members: crates/core, src-tauri
├── crates/
│   └── core/                     # ENGINE — framework-neutral (no tauri dep)
│       └── src/
│           ├── lib.rs            # Module exports
│           ├── api.rs            # The 20 operations as plain fns over a Ctx — both shells wrap these
│           ├── workspace.rs      # Session dirs + opaque artifact ids + reaper
│           ├── creds.rs          # RE NXT Connection model, on-disk format, token exchange/refresh
│           ├── profile.rs        # Bundle load/save/duplicate/create; all YAML structs
│           ├── validate.rs       # Profile linting (issue codes) + scaffold_missing
│           ├── db.rs             # DuckDB validation, SQL transforms, ResultSets, sql_path quoting
│           ├── re_calls.rs       # The one place a SKY API call is executed (Query + Code Table)
│           ├── code_tables.rs    # Pulls RE code tables into SQL; runs code_table_sync writes
│           │                     #   and publishes their outcomes as {{sync:Label}}
│           ├── query_step.rs     # Runs an re_query step: SQL params → RE query → JSON for later SQL
│           ├── user_input.rs     # Runs a user_input step: rows_sql → a form → the typed
│           │                     #   values published as {{form:Label}}
│           ├── report.rs         # Report pipeline: params → queries → transforms → result sets
│           └── errors.rs         # Shared AppError enum
│
├── src-tauri/                    # DESKTOP SHELL — Tauri binary
│   ├── Cargo.toml                # Depends on multitool-core + tauri
│   └── src/
│       ├── main.rs               # Entry point — registers all commands
│       ├── commands.rs           # #[tauri::command] wrappers: build Ctx from AppHandle, call core::api
│       └── sky_auth.rs           # RE NXT OAuth: loopback listener + system browser (desktop-only half)
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
Command bodies live in `crates/core/src/api.rs`; `commands.rs` only builds a `Ctx` and forwards.

**Profiles and imports** (`commands.rs` → `core::api`)

| Command | Called from (via `lib/api.ts`) | Args | Returns |
|---|---|---|---|
| `list_profiles` | `App.tsx` on mount — returns embedded built-ins (`builtin://<file>`) + `.import` files in `app_data_dir()/profiles/` (`user://<file>`) | _(none)_ | `ProfileSummary[]` |
| `load_profile` | `App.tsx` on profile select — mints a session, extracts the bundle into it | `zipPath` (a profile ref, see below) | `LoadedProfile` (carries `session_id` + `asset_base`) |
| `validate_file` | `App.tsx` on validate click | `filePath`, `inputLabel`, `sessionId` | `ValidationResult` |
| `run_profile` | `App.tsx` on generate click | `filePaths` (input label → local file path), `queryIds` (query label → artifact id), `syncIds` (sync label → artifact id), `formIds` (form label → artifact id), `sqlFile`, `sessionId`, `outputLabels` | `TransformResult` (outputs carry `artifact_id`) |
| `run_re_query` | `App.tsx` on an `re_query` step | `filePaths`, `stepLabel`, `sessionId` | `QueryStepResult` (carries `artifact_id`) |
| `run_code_table_sync` | `App.tsx` on a `code_table_sync` step | `filePaths`, `stepLabel`, `sessionId` | `SyncResult` (carries `artifact_id`) |
| `run_visualization` | `App.tsx` on a `visualization` step | `filePaths`, `queryIds`, `syncIds`, `formIds`, `stepLabel`, `sessionId` | `ResultSet` |
| `run_user_input` | `StepUserInput` on readiness and after each edit | `filePaths`, `queryIds`, `syncIds`, `formIds`, `stepLabel`, `sessionId`, `values` (row key → field id → value) | `UserInputResult` (carries `artifact_id`, `options`, `complete`) |
| `save_output` | `App.tsx` on download click (desktop Save As) | `sessionId`, `artifactId`, `destPath` | `void` |

**Reports** (`commands.rs` → `core::api` → `report.rs`; both `async`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `run_report` | `ReportsPage` Refresh | `sessionId`, `paramValues` | `ReportRunResult` (`data`, `queries`, `generated_at`, `mode`) |
| `run_report_action` | `ReportsPage` action button | `sessionId`, `actionId`, `paramValues` | `ActionResult` |

**In-app profile editor** (`commands.rs` → `profile.rs` / `validate.rs`)

| Command | Called from | Args | Returns |
|---|---|---|---|
| `new_profile` | Settings → Imports, "New profile" | _(AppHandle only)_ | `ProfileMutation` |
| `duplicate_profile` | Settings → Imports, "Duplicate" or opening a built-in | `sourceZipPath` | `ProfileMutation` |
| `save_profile` | Settings → Imports, Save | `zipPath` (a `user://` ref), `files` | `ProfileMutation` |
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

The pipeline commands (`validate_file`, `run_*`, `run_report*`) take
`sessionId` — the opaque handle `load_profile` returned — and the backend
re-reads `structure.yaml`, SQL, and fixtures from that session's `profile/`
dir. `save_profile` / `delete_profile` / `duplicate_profile` take a **profile
ref** instead (`builtin://<file>` or `user://<file>`), since they address the
bundle itself. No command accepts or returns a raw server path.

**Live vs mock.** Every RE-touching command picks a `Transport`: `Live` when a
connection exists (`sky_auth::has_connection`) and `RE_NXT_MOCK` is unset,
otherwise `Mock`, which reads the bundle's `fixtures/`. The chosen mode is
returned to the frontend and shown as a badge.

### Profile sources

- **Built-ins**: embedded at compile-time via `include_bytes!` in `crates/core/src/profile.rs::BUILTIN_PROFILES`. The `.import` files must exist when Rust builds — run `profiles/build.sh` first if you've changed a built-in's source.
- **User profiles**: `.import` files in `app_data_dir()/profiles/` — resolved via `AppHandle::path()`. macOS: `~/Library/Application Support/com.navin.tauri-import/profiles/`. Auto-created on first `list_profiles`.
- **`zip_path` shapes**: both are refs now — `user://<filename>` (resolved inside the user profiles dir, bare filenames only) and `builtin://<filename>` (extracted from in-memory bytes). Resolution + traversal checks live in `api.rs::resolve_profile_ref`.
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
`code_table_sync`, `user_input`, `visualization`, `manual_instruction`.
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
- `{{form:Label}}` — resolves to the values an earlier `user_input` step
  collected: one JSON object per row of the form, carrying `key`, every column
  its `rows_sql` selected, and one key per field id. The consumer must declare
  the label in `form_input` — a `sql_transform`, a `visualization`, or another
  `user_input` step whose own `rows_sql` depends on the earlier answers. Read
  with `read_json(..., columns={...})` — the result is empty when the form had
  no rows. See STEP_TYPES.md → *Step type: `user_input`*.
- `{{sync:Label}}` — resolves to the outcome rows an earlier `code_table_sync`
  step published via `sync_output` (one row per attempted write, carrying the id
  RE assigned). The consuming transform must declare the label in `sync_input`.
  Read with `read_json(..., columns={...})` — the result is empty when nothing
  needed syncing. See STEP_TYPES.md → *Step type: `code_table_sync`*.

Column names with spaces, `#`, `/` etc. must be double-quoted in SQL: `"Item #"`.

**Multi-line headers are flattened.** Where `{{input:Label}}` is the path
argument of a `read_*()` call, the whole call is rewritten to a projection that
renames every header to its flattened form — each run of whitespace collapsed to
one space, ends trimmed (`db::flattened_relation`). So a stacked Excel cell that
arrives as `"Award \r\n QTR/YR"` is quoted in SQL, and named in a `validation:`
label, as `"Award QTR/YR"`. Files whose headers are already clean are read
through the bare `read_*()` call, unchanged.

**Input reads are text-typed.** The same rewrite adds `all_varchar=true` to
every csv/xlsx input read (`db::with_all_varchar`), so a column arrives as the
characters the cell shows. Type inference silently corrupts identifiers: Excel
stores `946117` as a number, so the column comes back `DOUBLE` and casting it
to text yields `"946117.0"`, matching no id any API returns; CSV's sniffer
drops the zeros off `"011111111"`. Neither errors — the join just matches
nothing. The trade is that arithmetic on an input column needs an explicit cast
(`SUM(CAST("Amount" AS DOUBLE))`), which DuckDB demands with a binder error
naming the column. An author who wants the inferred types writes
`all_varchar=false` on the call.

---

## Frontend State Architecture

`App.tsx` owns all shared state (`files`, `generations`, `selectedProfile`)
and calls the backend exclusively through the typed functions in
`src/lib/api.ts` — no component imports `@tauri-apps/*` directly; `api.ts` is
the only file that does. Handlers (`handleFileSelect`, `handleValidate`,
`handleGenerate`, `handleDownload`, `handleSelectProfile`) live in `App.tsx`
and are passed down as props. Besides `App.tsx`, two settings components call
`api.ts` themselves: `settings/imports/ImportTab.tsx` (the profile editor) and
`settings/general/GeneralTab.tsx` (the RE NXT connection).

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
  - `imports/steps/StepVisualize.tsx` for `visualization` — source readiness
    row, then the rows drawn by the shared report `VIZ_REGISTRY`. Reads only;
    writes no file, and no progress bar. Self-refreshing: an effect runs the
    step whenever it is `idle` with every source ready, which is both the
    first-ready moment and every time App.tsx clears the result after an
    upstream change. The icon button is a manual re-read, not a gate.
  - `imports/steps/StepUserInput.tsx` for `user_input` — source readiness row,
    then one row of controls per row the step's `rows_sql` returned (or a single
    row when it has none), with every `rows_sql` column shown under its own
    header. Self-publishing like `StepVisualize`: it publishes on first
    readiness and again ~450ms after the last keystroke, so there is no button.
    Values live in `App.tsx` and survive re-derivation of the row list. A
    `select` field renders a plain dropdown under 8 choices and a searchable
    popover at 8+; its choices come from YAML `options` or, resolved per run,
    from the field's `options_sql`.
  - `imports/steps/StepImport.tsx` for `manual_instruction` — renders the
    markdown body with image assets resolved against `loadedProfile.asset_base`
    via `api.assetUrl` (asset protocol on desktop, session endpoint on web).
- `reports/ReportsPage.tsx` renders `ReportInputs` (from `parameters`) plus one
  component per `visualizations` entry, looked up in
  `reports/viz/index.tsx`'s `VIZ_REGISTRY` — the report-side mirror of
  `MainPanel`'s step dispatch. Only `TableViz` is implemented; the chart types
  render placeholders.
- `settings/imports/ImportTab.tsx` owns the in-app profile editor — it runs
  the editor commands itself and keeps its view children pure.

`loadedProfile.session_id` is threaded through `MainPanel` to each step
component and passed back to `validate_file`, `run_profile`, `run_re_query`,
`run_code_table_sync`, `run_user_input`, `run_visualization`, and `run_report`
so the backend can re-read validation rules, SQL, and fixtures from that session.

---

## Types

`src/types.ts` mirrors the Rust structs in `crates/core` (`profile.rs`, `db.rs`, `query_step.rs`, `code_tables.rs`, `user_input.rs`, `report.rs`) exactly. If you change a struct in Rust, update the matching type in `types.ts`. Import all types from `types.ts` — never inline them.

---

## Error Handling

- All Rust errors flow through `AppError` in `crates/core/src/errors.rs`
- Shell wrappers convert `AppError` to `String` for the frontend via `.map_err(|e| e.to_string())`
- Every `api.ts` call in TypeScript must be wrapped in `try/catch` — rejections carry the Rust error string (identical on the invoke and fetch paths)

---

## Style Rules

### Rust
- All functions that can fail return `Result<T, AppError>`
- Any path spliced into SQL text goes through `db::sql_path` (normalizes `\`→`/` and doubles embedded quotes)
- Engine code (`crates/core`) must never import `tauri` — shell concerns stay in the shells

### TypeScript / React
- Always `await` every `api.ts` call — forgetting causes silent failures
- New backend operations go in `core/src/api.rs` + a thin wrapper in each shell + a typed fn in `src/lib/api.ts`
- State shared across components lives in `App.tsx`
- File paths come from Tauri's file dialog APIs — do not manipulate them in JS

### General
- Never refactor code unless explicitly asked
- Make minimal changes — only touch the files relevant to the task

---

## Common Pitfalls

- **Forgot to register a command?** → Check `generate_handler![]` in `main.rs`
- **Backend call silently failing?** → Confirm the command is registered in `generate_handler![]` and you `await`-ed the call
- **DuckDB column error?** → Wrap column names with special characters in double
  quotes. For a stacked Excel header, quote the *flattened* name (`"Award QTR/YR"`)
- **Windows path backslash in SQL?** → Replace `\` with `/` in `db.rs` before string substitution
- **Excel header not on row 1?** → Add `OFFSET 1` or use a CTE in the profile SQL
- **`SUM`/`>` on an input column failing to bind?** → input columns are text by
  design (see *Input reads* below). Cast explicitly:
  `SUM(CAST("Amount" AS DOUBLE))`
- **State read too early?** → Use the value returned by the setter callback, not the stale state variable
- **Pipeline command failing with "Session not found"?** → the session was reaped (24h idle) or the backend restarted; reload the profile to mint a new one
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
- **`{{form:X}}` unresolved at runtime?** → same rule again with `form_input`
  and a `user_input` step declaring `form_output: X`
- **`{{form:X}}` joins to nothing?** → a required box on that row is still
  blank, so the step published `null` for it. The step stays "not done" and
  every consumer stays disabled until every required box on every row is filled
- **A select field shows "(not in list)"?** → the held value is no longer one of
  the field's options, because `options_sql` now returns a different set. The
  value is kept and published but marked stale, which blocks `complete` — pick
  again, or re-run whatever produces the options
- **`{{rows:col}}` sent as a string instead of an array?** → it only becomes an
  array when the YAML value is *exactly* that placeholder, e.g.
  `filter_values: "{{rows:record_id}}"`, not embedded in a longer string
- **Two profiles with the same name in the sidebar?** → built-in and user profile share an `id`; this is expected. Select on `zip_path`, never on `id`
- **Edited a built-in profile's source and nothing changed?** → run
  `./profiles/build.sh` and restart `npm run tauri dev`; built-ins are embedded
  at compile time via `include_bytes!` (in `crates/core/src/profile.rs`)
- **Report chart renders as an empty dashed box?** → only `table` is implemented
  in `VIZ_REGISTRY`; `bar`/`line`/`pie`/`kpi` are placeholder stubs. A
  `visualization` step in an import profile draws through the same registry, so
  the same limit applies
- **Report profile passes in-app Validate but fails `build.sh`?** → `validate.rs`
  implements the import rules; the report sections are only checked by `build.sh`

---

## What NOT to Do

- Do not modify `build.rs` — it is the Tauri build script
- Do not import `@tauri-apps/*` outside `src/lib/api.ts` — platform branching lives there
- Do not add Tauri (or Axum) dependencies to `crates/core` — it must build shell-free
- Do not put raw server paths in wire types — use `session_id` + artifact ids
- Do not put file I/O or DuckDB logic in React components
- Do not put UI state management in Rust commands
- Do not manipulate Windows file paths in JavaScript
- Do not add a Cargo dependency without checking if it's already available (slow recompile)
