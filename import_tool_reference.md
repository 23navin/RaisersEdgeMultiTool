# Import Tool — Developer Reference

> **Purpose of this document:** A single reference for everything you need while building
> and maintaining **the app itself**. Covers project goals, architecture decisions, Tauri
> concepts, file structure, the development cycle, and how data flows through the app.
>
> **Writing a profile, not app code?** Go to
> **[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)** instead — it is the complete
> authoring guide for both import and report profiles, with
> [STEP_TYPES.md](STEP_TYPES.md) and [REPORT_PROFILES.md](REPORT_PROFILES.md) as
> its field-level references.

---

## Table of Contents

1. [Project Goals](#1-project-goals)
2. [Technology Decisions](#2-technology-decisions)
3. [The Two Shells](#3-the-two-shells)
4. [The Engine and Its Shells](#4-the-engine-and-its-shells)
5. [Project File Structure](#5-project-file-structure)
6. [Profile Bundle Format](#6-profile-bundle-format)
7. [Data Flow — End to End](#7-data-flow--end-to-end)
8. [The Transport Layer — How Frontend Talks to Backend](#8-the-transport-layer--how-frontend-talks-to-backend)
9. [Development Cycle](#9-development-cycle)
10. [Building and Distributing](#10-building-and-distributing)
11. [Key Dependencies](#11-key-dependencies)
12. [Error Handling Strategy](#12-error-handling-strategy)
13. [Adding a New Feature — Decision Checklist](#13-adding-a-new-feature--decision-checklist)
14. [Common Pitfalls](#14-common-pitfalls)
15. [Glossary](#15-glossary)

---

## 1. Project Goals

### What this app does

A lightweight Windows desktop tool with two workspaces:

- **Imports** — transforms vendor-supplied CSV and Excel files into the specific column
  format and structure required by a target database's import tool, optionally enriching
  them with live lookups against the RE NXT (Blackbaud SKY) API and pushing new code-table
  entries back.
- **Reports** — runs parameterized queries against that same API and renders the results
  on screen.

### Core requirements

| Requirement | Decision |
|---|---|
| Works on Windows, ships as a single exe | Tauri |
| Modern, polished UI | React + Tailwind + shadcn/ui |
| Fast startup, low memory | Tauri (Rust binary, not Electron) |
| Profiles easy to write and hand off | YAML + SQL + Markdown zip bundles |
| Data transformation logic | DuckDB SQL |
| No runtime dependency on end user machine | Tauri compiled binary |
| Profile authors don't touch app code | Profile bundles are external zip files |

### What a "profile" is

A profile is a self-contained zip bundle (extension `.import`) that teaches the app how to
handle one specific job — one vendor's file format, or one report. Adding either means
shipping a new bundle: no recompiling, no code changes.

### Who writes profiles

Anyone comfortable writing SQL. A bundle is YAML metadata + SQL files + a Markdown
walkthrough; no programming language is required, and profiles can be written and
validated inside the app itself (Settings → Imports). See
[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md).

---

## 2. Technology Decisions

### Why Tauri (not Electron, not Python)

**vs. Electron:** Same web frontend stack, but Tauri uses a Rust binary instead of
bundling a full Node.js runtime and Chromium browser. Result: ~5MB binary vs ~150MB,
instant startup vs 2–4 seconds, lower memory footprint.

**vs. Python + CustomTkinter:** Python is fast to develop in but produces slow-starting
PyInstaller exes and UI frameworks that look dated. The profile authoring concern (keeping
it accessible) is solved differently here — profiles use SQL instead of Python, which is
more universally known.

### Why DuckDB for transforms

DuckDB can read CSV and Excel files directly via SQL (`read_csv_auto()`, `read_xlsx()`),
run arbitrary transformations, and write output — all without loading data into application
memory first. A profile's entire transformation logic is one SQL query. This makes profiles
readable, writable, and testable independently of the application.

### Why YAML + SQL + MD as the profile format

- **YAML** is human-readable, widely understood, has no runtime footprint
- **SQL** is the most transferable data language — analysts know it
- **Markdown** documentation travels with the profile and is readable anywhere
- **Zip** keeps the three files together as one unit, easy to share and version

### Why React + Tailwind + shadcn/ui

- React's component model maps cleanly to the three-step UI (pick profile → drop file → run)
- Tailwind removes the need for custom CSS files
- shadcn/ui provides accessible, well-designed components that look professional out of the box
- The entire frontend is just a web page — any web developer can contribute

---

## 3. The Two Shells

The app ships in two forms from one codebase, and the difference is confined to
a thin layer at the very edge.

**Tauri (desktop)** is a framework for building desktop applications where the
user interface is a web page, the application logic is a compiled Rust binary,
and the window is provided by the operating system's built-in web renderer
(WebView2 on Windows, which ships with Windows 11 and auto-installs on Windows
10). The web page is bundled inside the binary — no separate server, no network
connection, no browser installation.

**Axum (web)** is a Rust HTTP server. It serves the very same built web page as
static files and exposes the very same operations as HTTP endpoints. Users
reach it in an ordinary browser.

### What Tauri is NOT

- It is not a browser extension
- It is not Electron (no bundled Chromium, no Node.js runtime)
- The UI is not a native Windows UI — it is a web page rendered in a WebView

---

## 4. The Engine and Its Shells

This is the single most important concept to internalize. Almost none of the
code is desktop-specific or web-specific; nearly all of it lives in an engine
that knows about neither.

```
┌──────────────────────────────────────────────────────────────┐
│  FRONTEND — React + TypeScript (src/)                        │
│  Runs in the desktop WebView or an ordinary browser.         │
│  Handles: UI, user interaction, display.                     │
│  CANNOT: touch the filesystem or run DuckDB.                 │
└────────────────────────────┬─────────────────────────────────┘
                             │
                   src/lib/api.ts  — the ONE file that
                   knows which target it is running in
                             │
              ┌──────────────┴──────────────┐
     invoke() │                             │ fetch('/api/…')
              ▼                             ▼
┌──────────────────────────┐   ┌──────────────────────────────┐
│  DESKTOP SHELL           │   │  WEB SHELL                   │
│  src-tauri/              │   │  crates/server/              │
│  #[tauri::command] fns   │   │  POST /api/<command>         │
│  native dialogs          │   │  upload · download · assets  │
│  loopback OAuth listener │   │  OAuth callback route        │
└────────────┬─────────────┘   └───────────────┬──────────────┘
             │                                 │
             └────────────────┬────────────────┘
                              ▼
          ┌────────────────────────────────────────┐
          │  ENGINE — crates/core/                 │
          │  api.rs: every operation as a plain fn │
          │  profile parsing · DuckDB · validation │
          │  SKY API calls · sessions · creds      │
          │  NO tauri, NO http — pure Rust         │
          └────────────────────────────────────────┘
```

Each shell does three things and nothing more: build a `Ctx` (where state lives
on disk, how to reach RE), call the matching function in `core::api`, and
translate the result into its transport. The engine cannot tell which shell
invoked it.

**Rule of thumb:**
- Does it touch a file, database, or system resource? → **`crates/core`**, as a
  plain function in `api.rs`
- Is it transport- or platform-specific (dialogs, routes, OAuth redirect
  mechanics)? → **the shell**
- Does it change what the user sees? → **React**
- Does it need both? → the component calls a typed function in
  `src/lib/api.ts`, which reaches the backend the right way for its target

### The `Ctx`

```rust
pub struct Ctx {
    pub workspaces_root: PathBuf,     // parent of all session dirs
    pub user_profiles_dir: PathBuf,   // where user .import bundles live
    pub transport: Box<dyn Fn() -> Result<Transport, AppError> + Send + Sync>,
}
```

That closure is the whole of what the engine knows about credentials. Desktop
resolves it from the per-user connection file via `sky_auth`; the server
resolves it from its shared connection store. Either way the engine receives a
`Transport::Live { access_token, subscription_key }` or `Transport::Mock`.

### Sessions and artifact ids

Because the same code serves one desktop user and many concurrent web users,
nothing may be keyed on a shared name and no server path may cross the wire.

`load_profile` mints a **session** — a directory under `workspaces_root` — and
extracts the bundle into it. The client receives an opaque `session_id` and
echoes it on every later call. Files produced by steps are addressed by
**artifact id**: a session-relative path like `runs/18d0…/import_file.csv`.
`Workspace::resolve` re-anchors every id inside its own session and rejects
`..`, absolute paths, and backslashes.

```
<workspaces_root>/<session_id>/
├── profile/          extracted bundle (structure.yaml, sql/, fixtures/)
├── inputs/           uploaded files (web) — desktop reads local paths directly
├── queries/          re_query results        → {{query:Label}}
├── codetables/       code table pulls + sync outcomes → {{codetable:}} / {{sync:}}
├── forms/            user_input values, one file per label → {{form:Label}}
│                     rewritten in place on each edit, not per-run
└── runs/<token>/     one dir per transform run → output CSVs
```

Sessions hold **no in-memory state**. Every call re-reads `structure.yaml` and
the SQL from the session directory, which is why a stateless HTTP handler and a
Tauri command can share one implementation. Sessions idle for 24 hours are
reaped on the next `load_profile`.

---

## 5. Project File Structure

A Cargo workspace. The engine is a library crate; each shell is its own crate.

```
tauri-import/
│
├── Cargo.toml                        WORKSPACE root — crates/core, crates/server, src-tauri
│
├── src/                              FRONTEND — one React app for both targets
│   ├── main.tsx                      React entry point — mounts <App /> into index.html
│   ├── App.tsx                       Root component — holds shared state, calls lib/api.ts
│   ├── types.ts                      TS mirror of the Rust structs
│   ├── lib/
│   │   ├── api.ts                    THE TRANSPORT — invoke() vs fetch(); file pick,
│   │   │                             save, asset URLs; the only @tauri-apps importer
│   │   ├── utils.ts                  cn() helper
│   │   ├── profile-utils.ts          Profile summary/list helpers
│   │   └── re-calls.ts               TS mirror of the RE call registry
│   └── components/
│       ├── Titlebar.tsx              Window chrome + workspace tabs
│       ├── imports/                  Imports workspace (Sidebar, MainPanel, steps/)
│       ├── reports/                  Reports workspace (ReportInputs, viz/ registry)
│       ├── data-request/             Data Requests workspace
│       ├── settings/                 Settings, incl. the in-app profile editor
│       ├── shared/                   CodeMirror editor, notice block, panel
│       └── ui/                       Shadcn primitives
│
├── crates/
│   ├── core/                         THE ENGINE — no tauri, no http
│   │   └── src/
│   │       ├── lib.rs                Module exports
│   │       ├── api.rs                Every operation as a plain fn over a Ctx
│   │       ├── workspace.rs          Session dirs, artifact ids, containment, reaper
│   │       ├── creds.rs              RE NXT Connection model, storage, token exchange
│   │       ├── profile.rs            Bundle load/save/duplicate/create; the YAML structs
│   │       ├── validate.rs           Profile linting + missing-file scaffolding
│   │       ├── db.rs                 DuckDB execution — validation, transforms, result sets
│   │       ├── re_calls.rs           The single SKY API executor (Live / Mock transports)
│   │       ├── code_tables.rs        Code table pulls + code_table_sync writes
│   │       ├── query_step.rs         The re_query step runner
│   │       ├── user_input.rs         The user_input step runner (forms → {{form:…}})
│   │       ├── report.rs             The report pipeline
│   │       └── errors.rs             Shared error enum used across all modules
│   │
│   └── server/                       WEB SHELL — Axum
│       └── src/
│           ├── main.rs               Routes, uploads, downloads, OAuth callback, SPA
│           └── creds.rs              Server-wide RE connection store + refresh cache
│
├── src-tauri/                        DESKTOP SHELL — Tauri
│   ├── Cargo.toml                    Depends on multitool-core + tauri
│   ├── tauri.conf.json               Tauri configuration (window, permissions, bundle)
│   ├── build.rs                      Tauri build script — do not modify
│   └── src/
│       ├── main.rs                   Entry point — starts app, registers commands
│       ├── commands.rs               #[tauri::command] wrappers over core::api
│       └── sky_auth.rs               Loopback OAuth listener + system browser
│
├── Dockerfile, docker-compose.yml    Web server image + one-volume deployment
│
├── profiles/                         PROFILE BUNDLES — external, not compiled in
│   ├── src/<name>/                   Source: structure.yaml, instructions.md, sql/,
│   │                                 fixtures/, assets/, test-files/
│   ├── build.sh                      Verifies each source folder, packs <name>.import
│   └── <name>.import                 The zip the app consumes
│
├── API reference/                    SKY OpenAPI specs + query-synchronize best practices
├── index.html                        HTML shell that React mounts into
├── package.json                      Frontend dependencies (React, Tailwind, shadcn)
├── vite.config.ts                    Frontend build configuration
└── tsconfig.json                     TypeScript configuration
```

### File responsibilities at a glance

| File | Owns | Never touches |
|---|---|---|
| `App.tsx` | Shared state (selected profile, files, generations, params) | Filesystem, DuckDB |
| `imports/MainPanel.tsx` | Step dispatch — one component per `step.type` | Backend calls |
| `reports/viz/index.tsx` | `VIZ_REGISTRY` — one component per `visualization.type` | Backend calls |
| `settings/imports/ImportTab.tsx` | The profile editor's state + its own editor calls | Import/report execution |
| `lib/api.ts` | Choosing invoke() vs fetch(); platform-shaped file pick / save / asset URLs | Business logic |
| `core/api.rs` | Every operation; resolving ids → paths at the boundary | Transport, UI state |
| `core/workspace.rs` | Session dirs, artifact ids, containment checks, reaping | Profile semantics |
| `core/creds.rs` | Connection model, on-disk format, token exchange + refresh | How the code is obtained |
| `commands.rs` (shell) | Tauri command definitions; Ctx from AppHandle | Business logic |
| `server/main.rs` (shell) | Routes, multipart, downloads, OAuth callback | Business logic |
| `profile.rs` | Zip read/write, YAML parsing, instruction splitting | DuckDB, HTTP |
| `validate.rs` | Structural linting of a bundle, stub generation | Disk I/O |
| `db.rs` | DuckDB connection, SQL execution, CSV output | Profile parsing, HTTP |
| `re_calls.rs` | Every SKY HTTP call, job polling, fixtures | DuckDB |
| `report.rs` | Params → queries → transforms → result sets | HTTP details |
| `errors.rs` | Error type definitions | Everything else |

---

## 6. Profile Bundle Format

> This is the orientation version. The authoritative, field-by-field docs are
> [PROFILE_AUTHORING.md](PROFILE_AUTHORING.md) (how to write one),
> [STEP_TYPES.md](STEP_TYPES.md) (import step types), and
> [REPORT_PROFILES.md](REPORT_PROFILES.md) (report sections).

Each profile is a zip archive with the extension `.import`:

```
vendor_name.import
├── structure.yaml    REQUIRED — metadata + the whole declarative contract
├── instructions.md   On-screen prose, split per step by <!-- label: X --> anchors
├── sql/              The .sql files structure.yaml names
├── fixtures/         Canned RE API responses for offline ("mock") runs
└── assets/           Images referenced from instructions.md
```

There are two kinds, distinguished by a top-level `kind:` key:

| | Import profile (no `kind:`) | Report profile (`kind: report`) |
|---|---|---|
| Sections | `inputs`, `outputs`, `steps` | `parameters`, `queries`, `transforms`, `visualizations`, `actions` |
| Source data | Files the user uploads | The RE NXT (SKY) API |
| Result | CSVs the user downloads | On-screen visualizations |

Both kinds may declare a top-level `code_tables:` section, and both run their
SQL through the same DuckDB engine.

### structure.yaml

```yaml
id: vendor_a
name: "Vendor A Import"
version: "1.0"
min_app_version: "0.1.0"

inputs:
  - label: Classification
    type: csv                # csv | xlsx
    required: true
    validation:              # checked by the Validate button, before any SQL
      - { label: "Item #", required: true, type: number, digits: 6 }
      - { label: Category, required: true, type: string, value: ["Alpha", "Beta"] }

outputs:
  - label: Import_File
    type: csv

steps:
  - label: AddSourceFiles
    type: file_input
    input:
      - { label: Classification, validate: true }

  - label: CreateImportFile
    type: sql_transform
    input: [Classification]
    sql: primary_transform.sql
    output: [Import_File]

  - label: Import
    type: manual_instruction
```

Seven step types exist: `file_input`, `sql_transform`, `re_query`,
`code_table_sync`, `user_input`, `visualization`, `manual_instruction`.

### sql/*.sql

The SQL DuckDB executes. Files are named by the YAML and referenced through
placeholders — `{{input:Label}}` for an uploaded file, `{{output:Label}}` for a
declared output, `{{codetable:…}}` / `{{query:…}}` / `{{sync:…}}` for data
pulled from RE, and `{{form:…}}` for values a `user_input` step collected. (`{{input_file}}` is a legacy alias for the sole input of a
single-input transform.)

```sql
-- The SELECT defines the output columns and their order.
-- Column names here become the headers in the output CSV.

SELECT
    "Item #"                                        AS item_id,
    TRIM("Description")                             AS item_name,
    "Category"                                      AS category,
    CAST(
        REPLACE(REPLACE("Unit Cost", '$', ''), ',', '')
        AS DOUBLE
    )                                               AS unit_cost,
    "UOM"                                           AS unit_of_measure,
    COALESCE("Stock Qty", 0)                        AS quantity_on_hand

FROM read_csv_auto('{{input:Classification}}')

WHERE "Item #" IS NOT NULL
  AND TRIM("Item #") != ''

ORDER BY "Item #";
```

A bare `SELECT` like this is wrapped in a `COPY` to the single declared output.
To write several files from one transform, write your own
`COPY (...) TO '{{output:Label}}'` statements instead.

**DuckDB functions useful in profiles:**

| Function | What it does |
|---|---|
| `read_csv_auto('path')` | Reads a CSV, auto-detects types and delimiter |
| `read_xlsx('path')` | Reads an Excel file |
| `TRIM(col)` | Strips leading/trailing whitespace |
| `REPLACE(col, 'old', 'new')` | String replacement |
| `CAST(x AS DOUBLE)` | Type conversion |
| `COALESCE(col, default)` | Use default value when column is NULL |
| `UPPER(col)` / `LOWER(col)` | Case conversion |

### instructions.md

One Markdown file split into per-step sections by `<!-- label: StepLabel -->`
anchors. Text before the first anchor is the profile header. Document what the
profile does, where the source file comes from, any known quirks of the vendor's
format, and who to contact if it breaks.

### fixtures/

Canned RE responses so a profile that touches the API still runs with no
connection (or with `RE_NXT_MOCK=1`): `fixtures/<output>.json` for report
queries, `fixtures/queries/<query_output>.json` for `re_query` steps, and
`fixtures/codetables/<output>.json` for code tables.

### Where profiles come from

- **Built-ins** — embedded in the binary via `include_bytes!`
  (`BUILTIN_PROFILES` in `profile.rs`). Run `./profiles/build.sh` before the
  Rust build if you changed one.
- **User profiles** — `.import` files in `app_data_dir()/profiles/`, creatable
  and editable inside the app under Settings → Imports.

---

## 7. Data Flow — End to End

The trace below is identical on both targets except at the three marked
points. Component names are the real ones in `src/components/`.

### Step 1 — App starts

```
Desktop: main.rs starts Tauri  │  Web: browser loads the SPA from the server
  → React mounts, App.tsx runs api.listProfiles()
  → lib/api.ts picks invoke("list_profiles") or POST /api/list_profiles
  → core::api::list_profiles: embedded built-ins + .import files in the
    user profiles dir, each summarised from its structure.yaml
  → returns ProfileSummary[] — zip_path is a REF ("builtin://x.import"
    or "user://x.import"), never a filesystem path
  → imports/Sidebar.tsx populates the picker
```

### Step 2 — User selects a profile

```
Sidebar → App.tsx handleSelectProfile(zipPath)
  → api.loadProfile(zipPath)
  → core::api::load_profile
      Workspace::create()          mint <root>/<session_id>/
      extract bundle              → <session_id>/profile/
      load_from_dir               parse structure.yaml, instructions.md, sql/
      stamp session_id + asset_base
  → LoadedProfile crosses the wire WITHOUT temp_dir; the client holds
    session_id and echoes it on every later call
  → imports/MainPanel.tsx renders one section per step
```

### Step 3 — User attaches a file  ← differs by target

```
StepSelectFiles → api.pickInputFile(label, extensions, sessionId)

  DESKTOP: native dialog returns a local OS path; the backend reads it in place
  WEB:     <input type=file> → POST /api/sessions/{sid}/inputs (multipart)
           → server writes <session_id>/inputs/<token>-<name>
           → returns { path: "inputs/<token>-<name>" }, an artifact id

Either way App.tsx stores { path, name, status: "pending" } and the rest of
the pipeline is byte-identical.

Validate → api.validateFile(path, label, sessionId)
  → core::api::validate_file → db::validate_file
      fresh in-memory DuckDB, DESCRIBE over read_csv_auto/read_xlsx
      check required columns, nulls, allowed values, digit constraints
  → ValidationResult { ok, errors, notices } → inline error table
```

### Step 4 — User runs a transform

```
StepGenerateFile → App.tsx handleGenerate
  collects filePaths (input label → path/id)
           queryIds  (query_output label → artifact id from an earlier re_query)
           syncIds   (sync_output label  → artifact id from an earlier sync)
           formIds   (form_output label  → artifact id from an earlier user_input)
  → api.runProfile({...})
  → core::api::run_profile
      open_session               re-read structure.yaml + SQL from the session
      resolve every id           → absolute paths, containment-checked
      code_tables::fetch_all     if the profile declares any  → {{codetable:}}
      Workspace::new_run_dir()   fresh <session_id>/runs/<token>/
      db::run_transform          substitute {{input:}} {{output:}} {{query:}}
                                 {{sync:}} {{form:}} {{codetable:}}, run DuckDB,
                                 write one CSV per declared output
      relativize output paths    → artifact ids
  → TransformResult { outputs: [{label, artifact_id, row_count}], notices }
```

### Step 5 — User downloads the output  ← differs by target

```
StepGenerateFile → api.saveOutputFile(sessionId, artifactId, "Label.csv")

  DESKTOP: native Save As dialog → save_output command → fs::copy
  WEB:     GET /api/sessions/{sid}/artifacts/{id}?name=Label.csv
           → server streams it with Content-Disposition
```

The third target-specific point is **instruction images**
(`StepImport.tsx` → `api.assetUrl`): desktop resolves through Tauri's asset
protocol, web through `GET /api/sessions/{sid}/assets/{rel}`.

### How errors surface at each stage

| Stage | Example error | Where it's caught | What the user sees |
|---|---|---|---|
| Profile load | Zip is corrupt, or an entry escapes the bundle | `core/profile.rs` | "Cannot read zip archive: …" / "…has an unsafe path" |
| Session | Reaped after 24h idle, or unknown id | `core/workspace.rs` | "Session '…' not found — reload the profile and try again" |
| Artifact id | `..`, absolute path, or backslash | `core/workspace.rs` | "Invalid artifact id: …" |
| File validation | Missing required column | `core/db.rs` | Inline table: "Missing expected column: Item #" |
| SQL execution | Type cast fails | `core/db.rs` | "Transform failed: could not cast 'N/A' to DOUBLE" |
| Declared output not written | SQL never wrote `{{output:X}}` | `core/db.rs` | "Output 'X' was declared but the SQL did not write to it" |
| RE call | Token expired, SKY 4xx/5xx | `core/re_calls.rs` | "Network error: SKY API returned 401 …" |
| Auth | No connection, spent refresh token | `core/creds.rs` | "Not connected…" / "Token endpoint returned 400: …" |

---

## 8. The Transport Layer — How Frontend Talks to Backend

Components never call `invoke()` or `fetch()` directly. They import a typed
function from `src/lib/api.ts`, which is the only file in `src/` allowed to
import from `@tauri-apps/*`.

### Calling the backend from a component

```typescript
import * as api from "../../lib/api";

const result = await api.runProfile({
  filePaths, queryIds, syncIds, formIds,
  sqlFile: transform.sql,
  sessionId: loadedProfile.session_id,
  outputLabels: transform.output ?? [],
});
```

### How the transport picks a target

```typescript
// src/lib/api.ts
export const isTauri = "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  }
  const resp = await fetch(`/api/${command}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {}),
  });
  if (!resp.ok) throw await resp.text();   // same string a rejected invoke() carries
  return (await resp.json()) as T;
}
```

Because Tauri encodes command arguments as camelCase JSON and the server's
request structs use `#[serde(rename_all = "camelCase")]`, **the two paths send
byte-identical bodies**. That is what lets one `dist/` serve both targets.

### Defining an operation — all three layers

**1. The engine** (`crates/core/src/api.rs`) — where the work actually happens:

```rust
pub fn run_profile(
    ctx: &Ctx,
    file_paths: HashMap<String, String>,
    query_ids: HashMap<String, String>,
    /* … */
) -> Result<TransformResult, AppError> { /* … */ }
```

**2a. The desktop shell** (`src-tauri/src/commands.rs`):

```rust
#[tauri::command]
pub async fn run_profile(app: AppHandle, /* … */) -> Result<TransformResult, String> {
    let ctx = ctx(&app)?;
    // Off the async runtime — the live path uses reqwest::blocking,
    // which panics inside a Tokio context.
    tokio::task::spawn_blocking(move || {
        api::run_profile(&ctx, /* … */).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}
```

**2b. The web shell** (`crates/server/src/main.rs`):

```rust
async fn run_profile(
    State(s): State<Arc<AppState>>,
    Json(req): Json<RunProfileReq>,
) -> ApiResult<TransformResult> {
    let _permit = s.run_permits.acquire().await.expect("semaphore open");
    let ws = s.open_workspace(&req.session_id)?;
    let file_paths = resolve_inputs(&ws, &req.file_paths)?;   // ids → paths
    let ctx = s.ctx();
    Ok(Json(blocking(move || api::run_profile(&ctx, file_paths, /* … */)).await?))
}
```

**3. The frontend** (`src/lib/api.ts`) — one typed function, as above.

### Registering commands

Desktop commands must appear in `generate_handler![]` in
`src-tauri/src/main.rs`, or `invoke()` fails silently. Server routes must be
added to the `Router` in `crates/server/src/main.rs`. **Adding an operation
means touching all three layers** — engine, both shells, transport module.

### The command surface

| Command | Args | Returns |
|---|---|---|
| `list_profiles` | — | `ProfileSummary[]` |
| `load_profile` | `zipPath` (a `builtin://` / `user://` ref) | `LoadedProfile` |
| `validate_file` | `filePath`, `inputLabel`, `sessionId` | `ValidationResult` |
| `run_profile` | `filePaths`, `queryIds`, `syncIds`, `sqlFile`, `sessionId`, `outputLabels` | `TransformResult` |
| `run_re_query` | `filePaths`, `stepLabel`, `sessionId` | `QueryStepResult` |
| `run_code_table_sync` | `filePaths`, `stepLabel`, `sessionId` | `SyncResult` |
| `run_visualization` | `filePaths`, `queryIds`, `syncIds`, `stepLabel`, `sessionId` | `ResultSet` |
| `run_report` | `sessionId`, `paramValues` | `ReportRunResult` |
| `run_report_action` | `sessionId`, `actionId`, `paramValues` | `ActionResult` |
| `save_output` *(desktop only)* | `sessionId`, `artifactId`, `destPath` | `void` |
| `save_profile` | `zipPath`, `files` | `ProfileMutation` |
| `new_profile` | — | `ProfileMutation` |
| `duplicate_profile` | `sourceZipPath` | `ProfileMutation` |
| `delete_profile` | `zipPath` | `void` |
| `validate_profile` | `files` | `ValidationReport` |
| `scaffold_missing` | `files` | `ProfileFileEntry[]` |
| `re_nxt_status` | — | `ConnectionStatus` |
| `connect_re_nxt` | `clientId`, `clientSecret`, `subscriptionKey` | desktop `ConnectionStatus`; web `{authorizeUrl, redirectUri}` |
| `disconnect_re_nxt` | — | `void` |

`connect_re_nxt` is the one operation whose *shape* differs: desktop blocks
until the loopback handshake finishes and returns the final status, whereas the
web version returns a URL to navigate to and completes at
`GET /api/oauth/callback`. `api.ts` hides the difference behind one function.

The web shell also exposes upload, download, asset, and OAuth-callback routes
that have no desktop equivalent — see **[SERVER.md](SERVER.md) §9**.

---

## 9. Development Cycle

### Starting the dev environment

**Desktop:**

```bash
npm run tauri dev
```

This single command compiles the Rust backend, starts the Vite dev server, and
opens a live window.

**Web server:**

```bash
npm run build                     # the server serves dist/, not Vite's dev server
cargo run -p multitool-server
# → http://localhost:8080
```

For fast UI iteration against the server, run Vite separately and point it at
the backend — add a proxy to `vite.config.ts` (`server.proxy['/api'] →
http://localhost:8080`) and use `npm run dev`. Otherwise re-run `npm run build`
after frontend edits.

Most work needs only one target: the engine is shared, so a change to
`crates/core` is exercised by whichever shell is convenient. `cargo test
--workspace` covers the engine without either.

### Hot reload behavior

| What you change | What happens |
|---|---|
| `.tsx` / `.ts` file | Desktop: reloads instantly. Server: re-run `npm run build` (or proxy Vite) |
| `.css` / Tailwind class | Same as above |
| `.rs` in `crates/core` | Both shells recompile (5–30 seconds) |
| `.rs` in a shell | Only that shell recompiles |
| `Cargo.toml` (new dependency) | Full recompile, slower |
| `tauri.conf.json` | Restart `npm run tauri dev` |

### Practical workflow tip

Because Rust recompiles are slow, structure your work to:
1. **Get Rust logic stable first** — commands defined, DuckDB wired, errors handled
2. **Then iterate freely on the frontend** — layout, styling, component behavior all hot-reload instantly

### Checking Rust errors

Rust compiler errors appear in the terminal where you ran `npm run tauri dev`.
They are verbose but precise — they tell you exactly what line, why it's wrong,
and often suggest the fix. Read them fully before searching.

### Checking frontend errors

Errors appear in the browser devtools console. Open it with:
`View → Toggle Developer Tools` in the Tauri window, or add this to `tauri.conf.json`:

```json
"devtools": true
```

---

## 10. Building and Distributing

Two artifacts come out of one commit. Run `./profiles/build.sh` before either —
built-ins are embedded at compile time.

### Desktop — build command

```bash
npm run tauri build
```

### Output location

```
src-tauri/target/release/bundle/
├── msi/
│   └── ImportTool_1.0.0_x64_en-US.msi     Windows installer
└── nsis/
    └── ImportTool_1.0.0_x64-setup.exe      Standalone installer
```

The compiled binary at `src-tauri/target/release/import-tool.exe` also works as a
portable exe if you prefer not to use an installer.

### What ships to the end user

- The `.exe` or `.msi`. The built-in profiles listed in `BUILTIN_PROFILES` are
  compiled into it, so run `./profiles/build.sh` *before* `npm run tauri build`.
- Nothing else is required. Extra profiles are distributed as loose `.import`
  files.

### Where the app looks for profiles

Resolved at runtime via `AppHandle::path().app_data_dir()`, plus `profiles/`:

- Windows: `%APPDATA%\com.navin.tauri-import\profiles\`
- macOS: `~/Library/Application Support/com.navin.tauri-import/profiles/`

The directory is created on the first `list_profiles`. Dropping a `.import`
file there makes it available on the next launch, alongside the built-ins; the
in-app editor (Settings → Imports) writes to the same place. On the server the
equivalent directory is `DATA_DIR/profiles/`.

Profiles are addressed on the wire by **ref**, never by path:
`builtin://<filename>` for an embedded bundle, `user://<filename>` for one in
the profiles directory. `api.rs::resolve_profile_ref` rejects anything else,
including traversal attempts.

### Web server — build command

```bash
docker build -t multitool-server .
```

A two-stage build: Node compiles the SPA, Rust compiles `multitool-server`
(with DuckDB bundled from source), and the runtime image carries just the
binary, `dist/`, and a volume at `/data`. Build it directly instead with:

```bash
npm run build && cargo build --release -p multitool-server
```

### What ships where

| Target | Artifact | Carries |
|---|---|---|
| Desktop | `.msi` / `.exe` | Binary + embedded built-ins + `dist/` |
| Web | Container image | Binary + embedded built-ins + `dist/`, state on the `/data` volume |

CI builds both from the same tag — see `.github/workflows/build.yml`.
Deployment and configuration live in **[SERVER.md](SERVER.md)**.

---

## 11. Key Dependencies

### Engine — `crates/core/Cargo.toml`

Deliberately free of any framework. This is what makes the dual target
possible: `cargo tree -p multitool-core` contains **no tauri and no axum**.

| Crate | Purpose |
|---|---|
| `duckdb` (bundled) | Embedded analytics database — runs profile SQL in-process |
| `serde` + `serde_yaml` | Deserializing structure.yaml into Rust structs |
| `serde_json` | Fixtures, RE payloads, results to the frontend |
| `zip` | Reading and writing `.import` bundles |
| `reqwest` (blocking, rustls) | SKY API calls and the OAuth token endpoint |
| `chrono` | Timestamps |
| `getrandom` | CSRF nonce for the OAuth handshake |

### Desktop shell — `src-tauri/Cargo.toml`

| Crate | Purpose |
|---|---|
| `multitool-core` | The engine |
| `tauri` | Window, IPC, asset protocol |
| `tauri-plugin-dialog` | Native open/save dialogs |
| `tokio` | `spawn_blocking` for the blocking engine calls |

### Web shell — `crates/server/Cargo.toml`

| Crate | Purpose |
|---|---|
| `multitool-core` | The engine |
| `axum` (+ multipart) | HTTP routing, uploads |
| `tokio` (multi-thread) | Async runtime + the blocking pool |
| `tower-http` | Static file serving, tracing, body limits |
| `tracing` + `tracing-subscriber` | Structured logs |

### Frontend (package.json)

| Package | Purpose |
|---|---|
| `react` + `react-dom` | UI framework |
| `@tauri-apps/api` + `@tauri-apps/plugin-dialog` | `invoke()`, native dialogs — imported **only** by `src/lib/api.ts`, and dynamically, so a browser build never loads them |
| `tailwindcss` | Utility-first CSS |
| `@radix-ui/*` / `shadcn/ui` | Accessible, styled UI components |
| `@uiw/react-codemirror` | The in-app profile editor |
| `vite` | Frontend build tool and dev server |

---

## 12. Error Handling Strategy

### In Rust

All errors flow through a single `AppError` enum defined in `errors.rs`.
Every function that can fail returns `Result<T, AppError>`.
Commands convert `AppError` to `String` for the frontend.

```rust
// errors.rs
pub enum AppError {
    ProfileNotFound(String),
    InvalidFileType { got: String, expected: Vec<String> },
    MissingColumns(Vec<String>),
    SqlError(String),
    IoError(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::MissingColumns(cols) =>
                write!(f, "Missing expected columns: {}", cols.join(", ")),
            AppError::SqlError(msg) =>
                write!(f, "SQL error: {}", msg),
            // ...
        }
    }
}
```

### In TypeScript

`invoke()` returns a Promise that rejects on `Err(...)` from Rust.
Wrap every invoke call in try/catch:

```typescript
try {
  const result = await invoke<OutputResult>("run_profile", { ... });
  setStatus("success");
  setResult(result);
} catch (error) {
  setStatus("error");
  setErrorMessage(error as string);  // Rust's error string comes through here
}
```

### Principle

Errors should be caught at the source (in `db.rs` or `profile.rs`), enriched with
context, and surfaced to the user with enough detail to act on. Never let a panic
reach the user.

---

## 13. Adding a New Feature — Decision Checklist

When you want to add something, answer these questions:

**Does it read or write a file?**
→ Logic goes in `db.rs` or `profile.rs`, exposed via a new command in `commands.rs`

**Does it change what the user sees or interacts with?**
→ Logic goes in a React component in `src/components/`

**Does it need data from the backend to display something?**
→ Add a command in `commands.rs`, call it with `invoke()` from the component

**Is it a new step in the workflow?**
→ Consider whether it belongs in an existing component or needs a new one.
→ If it needs shared state with other components, lift that state to `App.tsx`.

**Is it a new profile capability?** (e.g. multi-sheet Excel, pre/post SQL hooks)
→ Extend the `profile.yaml` schema and `Profile` struct, handle in `profile.rs` and `db.rs`.
→ Frontend only needs updating if the user needs to configure it per-run.

---

## 14. Common Pitfalls

### Rust

**Forgetting to register a command**
If `invoke("my_command")` silently fails or returns an error like "command not found",
check that `commands::my_command` is listed in the `generate_handler![]` macro in `main.rs`.

**Borrow checker fighting you on strings**
When passing strings into DuckDB or file paths, you'll often need to convert between
`String`, `&str`, `Path`, and `PathBuf`. Use `.as_str()`, `.to_string()`, `Path::new()`,
and `.to_path_buf()` liberally — this is normal, not a sign something is wrong.

**Slow recompiles**
If you're making many small Rust changes, prefer a unit test in `crates/core`
(`cargo test -p multitool-core`) over rebuilding a whole shell. Avoid changing
`Cargo.toml` unnecessarily — adding dependencies triggers the slowest recompiles.

**Adding a framework dependency to `crates/core`**
The engine must build with neither shell present. Anything Tauri- or
HTTP-specific belongs in `src-tauri/` or `crates/server/`. Guard rail:
`cargo tree -p multitool-core` must list no `tauri` and no `axum`.

**Forgetting the other shell**
A new operation needs a wrapper in *both* `src-tauri/src/commands.rs` and
`crates/server/src/main.rs`, plus a typed function in `src/lib/api.ts`. Adding
it to one shell only means the feature silently doesn't exist on the other
target.

**Putting a server path in a wire type**
Anything returned to the client must be a `session_id` or an artifact id, never
an absolute path. A leaked path is both an information disclosure and a bug on
the web, where the client's filesystem is not the server's.

### Frontend

**File paths on Windows**
Windows paths use backslashes. When passing a file path from the frontend to Rust,
pass it as-is from the file picker — don't manipulate it in JavaScript.

**Importing `@tauri-apps/*` outside `src/lib/api.ts`**
It works on desktop and breaks in a browser. Every platform difference belongs
behind a function in the transport module.

**Backend call not awaited**
Every `api.ts` function returns a Promise. Forgetting `await` causes silent
failures where the UI moves on before the backend has responded.

**State update timing**
React state updates are asynchronous. If you set state and immediately read it in
the same function, you'll get the old value. Use the updated value from the setter
callback or restructure the flow.

### DuckDB / Profiles

**Column names with special characters**
DuckDB requires column names with spaces, #, /, or other special characters to be
wrapped in double quotes in SQL: `"Item #"` not `Item #`.

**Multi-line Excel headers**
A header typed as a stacked cell arrives as a column named `"Award \r\n QTR/YR"`,
which no author can quote reliably. `db::flattened_relation` wraps every input
read in a projection that renames such headers to their flattened form (runs of
whitespace collapsed to one space, ends trimmed), so SQL and `validation:` labels
both use `"Award QTR/YR"`. The rewrite fires where `{{input:Label}}` is the path
argument of a `read_*()` call; anywhere else the placeholder still resolves to a
plain path, just without flattening.

**Excel files with merged cells or header rows above row 1**
`read_xlsx()` assumes row 1 is the header. If the vendor file has a title row above
the headers, add `OFFSET 1` or handle it in a CTE within the SQL.

**Paths spliced into SQL**
Every path substituted into SQL text must go through `db::sql_path`, which
normalizes `\` to `/` (DuckDB accepts forward slashes on Windows) and doubles
embedded single quotes so a path containing `'` cannot terminate the literal
early. Never interpolate a raw path.

**A `{{form:X}}` or `{{sync:X}}` join failing to bind**
A sync that attempted zero rows publishes `[]`, so `read_json_auto` has no
schema to infer — the same is true of a form whose rows came back empty. Use
`read_json('{{sync:X}}', columns={…})` naming the columns
you join on.

---

## 15. Glossary

| Term | Definition |
|---|---|
| **Tauri** | Framework for building desktop apps with a web frontend and Rust backend |
| **Renderer process** | The web page / React app running inside the WebView |
| **Main process** | The Rust binary that owns the window and system access |
| **IPC** | Inter-process communication — how the frontend and backend talk |
| **invoke()** | Tauri's TypeScript function that calls a Rust command and returns a Promise — used only inside `src/lib/api.ts` |
| **Shell** | The thin per-target layer around the engine: `src-tauri` (Tauri) or `crates/server` (Axum) |
| **Engine** | `crates/core` — every operation as a plain function, framework-free |
| **Ctx** | What an operation needs from its host: state directories plus a transport resolver |
| **Session** | A per-load working directory; the client holds its opaque `session_id` |
| **Artifact id** | A session-relative path (e.g. `runs/…/out.csv`) standing in for a server path on the wire |
| **Profile ref** | `builtin://<file>` or `user://<file>` — how a bundle is addressed instead of by path |
| **Transport** | Either `Transport::Live { access_token, subscription_key }` or `Transport::Mock` (bundle fixtures) |
| **#[tauri::command]** | Rust attribute that marks a function as callable from the frontend |
| **WebView2** | Windows' built-in web renderer (like a lightweight browser engine) — used by Tauri |
| **DuckDB** | Embedded SQL database that reads files directly and runs analytical queries |
| **Profile bundle** | A `.import` zip containing `structure.yaml`, `instructions.md`, `sql/`, and optional `fixtures/` and `assets/` |
| **Import profile** | A profile with `inputs` / `outputs` / `steps` — uploads in, CSVs out |
| **Report profile** | A profile with `kind: report` — parameters in, live RE data on screen |
| **Step** | One entry in an import profile's `steps:` list; its `type` picks the UI component |
| **Placeholder** | A `{{…}}` token in profile SQL or YAML the runtime substitutes (`{{input:X}}`, `{{output:X}}`, `{{query:X}}`, `{{sync:X}}`, `{{form:X}}`, `{{codetable:X}}`, `{{param:X}}`) |
| **Fixture** | A canned RE API response in the bundle, used when running in mock mode |
| **Mock mode** | Fixture-backed execution — no RE connection, or `RE_NXT_MOCK=1` |
| **SKY / RE NXT** | Blackbaud's Raiser's Edge NXT API, the source of query and code-table data |
| **Cargo.toml** | Rust's dependency manifest file (equivalent to `package.json`) |
| **Crate** | A Rust library/package (equivalent to an npm package) |
| **Result<T, E>** | Rust's way of returning either a success value `T` or an error `E` |
| **serde** | Rust library for serializing/deserializing data (JSON, YAML, etc.) |
| **Vite** | The frontend build tool and dev server |
| **shadcn/ui** | A collection of accessible, styled React components built on Radix UI |
| **Hot reload** | Frontend changes appear instantly without restarting the app |
| **`tauri build`** | Compiles the entire app into a distributable Windows binary |
