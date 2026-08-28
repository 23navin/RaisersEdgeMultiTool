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
3. [What Tauri Is and How It Works](#3-what-tauri-is-and-how-it-works)
4. [The Two-Process Model](#4-the-two-process-model)
5. [Project File Structure](#5-project-file-structure)
6. [Profile Bundle Format](#6-profile-bundle-format)
7. [Data Flow — End to End](#7-data-flow--end-to-end)
8. [The IPC Bridge — How Frontend Talks to Backend](#8-the-ipc-bridge--how-frontend-talks-to-backend)
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

## 3. What Tauri Is and How It Works

Tauri is a framework for building desktop applications where:

- The **user interface** is a web page (HTML, CSS, JavaScript/TypeScript)
- The **application logic** is a compiled Rust binary
- The **window** is provided by the operating system's built-in web renderer
  (WebView2 on Windows, which ships with Windows 11 and is auto-installed on Windows 10)

When you run a Tauri app, the Rust binary starts, creates a window, and loads your
web page into it. The web page is bundled inside the binary — there is no separate
server, no network connection, no browser installation required.

### What Tauri is NOT

- It is not a browser extension
- It is not a web server
- It is not Electron (no bundled Chromium, no Node.js runtime)
- The UI is not a native Windows UI — it is a web page rendered in a WebView

---

## 4. The Two-Process Model

This is the single most important concept to internalize.

```
┌─────────────────────────────────────────────┐
│  RENDERER PROCESS (Frontend)                │
│                                             │
│  React + TypeScript                         │
│  Runs inside the WebView (like a browser)   │
│  Handles: UI, user interaction, display     │
│                                             │
│  CANNOT: touch filesystem, run DuckDB,      │
│           access system resources           │
│                                             │
│  COMMUNICATES via: invoke() calls           │
└──────────────────┬──────────────────────────┘
                   │   IPC Bridge
                   │   (inter-process communication)
                   │   invoke("command_name", { args })
                   │   ← returns Result<T, E>
┌──────────────────▼──────────────────────────┐
│  MAIN PROCESS (Backend)                     │
│                                             │
│  Rust binary                                │
│  Handles: files, DuckDB, profile parsing    │
│                                             │
│  CAN: read/write files, run DuckDB,         │
│        access system, spawn processes       │
│                                             │
│  EXPOSES via: #[tauri::command] functions   │
└─────────────────────────────────────────────┘
```

**Rule of thumb:**
- Does it touch a file, database, or system resource? → **Rust (backend)**
- Does it change what the user sees? → **React (frontend)**
- Does it need both? → **Frontend calls backend via invoke(), backend returns data, frontend displays it**

---

## 5. Project File Structure

```
tauri-import/
│
├── src/                              FRONTEND — React app
│   ├── main.tsx                      React entry point — mounts <App /> into index.html
│   ├── App.tsx                       Root component — holds shared state, makes the invoke() calls
│   ├── types.ts                      TS mirror of the Rust structs
│   ├── lib/                          cn() helper, profile helpers, RE call catalog
│   └── components/
│       ├── Titlebar.tsx              Window chrome + workspace tabs
│       ├── imports/                  Imports workspace (Sidebar, MainPanel, steps/)
│       ├── reports/                  Reports workspace (ReportInputs, viz/ registry)
│       ├── data-request/             Data Requests workspace
│       ├── settings/                 Settings, incl. the in-app profile editor
│       ├── shared/                   CodeMirror editor, notice block, panel
│       └── ui/                       Shadcn primitives
│
├── src-tauri/                        BACKEND — Rust binary
│   ├── Cargo.toml                    Rust dependencies (equivalent to package.json)
│   ├── tauri.conf.json               Tauri configuration (window, permissions, bundle)
│   ├── build.rs                      Tauri build script — do not modify
│   └── src/
│       ├── main.rs                   Entry point — starts app, registers commands
│       ├── commands.rs               #[tauri::command] functions (the backend API)
│       ├── profile.rs                Bundle load/save/duplicate/create; the YAML structs
│       ├── validate.rs               Profile linting + missing-file scaffolding
│       ├── db.rs                     DuckDB execution — validation, transforms, result sets
│       ├── re_calls.rs               The single SKY API executor (Live / Mock transports)
│       ├── code_tables.rs            Code table pulls + code_table_sync writes
│       ├── query_step.rs             The re_query step runner
│       ├── report.rs                 The report pipeline
│       ├── sky_auth.rs               RE NXT OAuth + credential storage
│       └── errors.rs                 Shared error enum used across all modules
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
| `settings/imports/ImportTab.tsx` | The profile editor's state + its own editor `invoke()`s | Import/report execution |
| `commands.rs` | Tauri command definitions (the API surface) | UI state |
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

Five step types exist: `file_input`, `sql_transform`, `re_query`,
`code_table_sync`, `manual_instruction`.

### sql/*.sql

The SQL DuckDB executes. Files are named by the YAML and referenced through
placeholders — `{{input:Label}}` for an uploaded file, `{{output:Label}}` for a
declared output, `{{codetable:…}}` / `{{query:…}}` / `{{sync:…}}` for data
pulled from RE. (`{{input_file}}` is a legacy alias for the sole input of a
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

### Step 1 — App starts

```
main.rs starts the Tauri app
  → WebView loads, React mounts
  → ProfilePicker calls invoke("list_profiles")
  → commands.rs → profile.rs scans the profiles/ folder
  → Returns list of { name, description, accepts } for each zip
  → ProfilePicker populates the dropdown
```

### Step 2 — User selects a profile and drops a file

```
User picks a profile from the dropdown
  → App.tsx stores selectedProfile

User drops or selects a file
  → FileDropZone calls invoke("validate_file", { filePath, profileName })
  → commands.rs → profile.rs loads the profile bundle
      → Checks file extension against profile.accepts
      → Reads the file header row
      → Checks all expected_columns are present
  → Returns Ok(ValidationResult) or Err(message)
  → FileDropZone shows green checkmark or red error message
  → If valid: Run button in ResultsPanel becomes enabled
```

### Step 3 — User clicks Run

```
ResultsPanel calls invoke("run_profile", { filePath, profileName, outputDir })
  → commands.rs →
      profile.rs: load profile bundle, extract SQL, replace {{input_file}}
      db.rs: open DuckDB connection
             execute the SQL query
             write result to output CSV at outputDir/prefix_timestamp.csv
             return row count
  → Returns Ok(OutputResult { outputPath, rowCount }) or Err(AppError)
  → ResultsPanel shows success (path + row count) or error with detail
```

### How errors surface at each stage

| Stage | Example error | Where it's caught | What user sees |
|---|---|---|---|
| Profile load | Zip is corrupt | `profile.rs` | "Could not load profile: ..." |
| File validation | Wrong extension | `commands.rs` | "This profile accepts .csv, .xlsx only" |
| Column check | Missing "Item #" | `commands.rs` | "Missing expected columns: Item #" |
| SQL execution | Type cast fails | `db.rs` | "SQL error: could not cast 'N/A' to DOUBLE in Unit Cost" |
| File write | Output dir read-only | `db.rs` | "Could not write output file: ..." |

---

## 8. The IPC Bridge — How Frontend Talks to Backend

### Calling a backend command from the frontend

```typescript
// src/components/ProfilePicker.tsx
import { invoke } from "@tauri-apps/api/core";

// invoke<ReturnType>("command_name", { arg1: value1, arg2: value2 })
// Returns a Promise — always use async/await or .then()/.catch()

const profiles = await invoke<ProfileMeta[]>("list_profiles", {
  profilesDir: "/path/to/profiles"
});
```

### Defining a command in Rust

```rust
// src-tauri/src/commands.rs

#[tauri::command]
pub fn list_profiles(profiles_dir: String) -> Result<Vec<ProfileMeta>, String> {
    profile::list(Path::new(&profiles_dir))
        .map_err(|e| e.to_string())   // convert AppError to String for the frontend
}
```

### Registering commands so Tauri knows about them

```rust
// src-tauri/src/main.rs

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_profiles,
            commands::validate_file,
            commands::run_profile,
        ])
        .run(tauri::generate_context!())
        .unwrap();
}
```

**Every command must be registered here or invoke() will fail silently.**

### The command surface

Grouped by area; the full arg/return table lives in [CLAUDE.md](CLAUDE.md#the-backend-commands).

| Area | Commands |
|---|---|
| Profiles + imports | `list_profiles`, `load_profile`, `validate_file`, `run_profile`, `run_re_query`, `run_code_table_sync`, `save_output` |
| Reports | `run_report`, `run_report_action` |
| In-app profile editor | `new_profile`, `duplicate_profile`, `save_profile`, `delete_profile`, `validate_profile`, `scaffold_missing` |
| RE NXT connection | `connect_re_nxt`, `re_nxt_status`, `disconnect_re_nxt`, `re_nxt_access_token` |

Note that the `zipPath` argument on the *run* commands is the extracted temp dir
(`loadedProfile.temp_dir`), not the `.import` zip — the editor commands are the
ones that take a real bundle path.

---

## 9. Development Cycle

### Starting the dev environment

```bash
npm run tauri dev
```

This single command:
1. Compiles the Rust backend
2. Starts the Vite dev server for the frontend
3. Opens a live window

### Hot reload behavior

| What you change | What happens |
|---|---|
| `.tsx` / `.ts` file | Frontend reloads instantly (< 1 second) |
| `.css` / Tailwind class | Frontend reloads instantly |
| `.rs` file | Rust recompiles, window restarts (5–30 seconds) |
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

### Build command

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
in-app editor (Settings → Imports) writes to the same place. Built-ins are
addressed by the sentinel `builtin://<filename>` rather than a filesystem path.

---

## 11. Key Dependencies

### Rust (src-tauri/Cargo.toml)

| Crate | Purpose |
|---|---|
| `tauri` | The framework — window, IPC, file dialogs |
| `duckdb` | Embedded analytics database — runs your SQL profiles |
| `serde` + `serde_yaml` | Deserializing profile.yaml into Rust structs |
| `zip` | Unpacking profile .zip bundles |
| `serde_json` | Serializing results back to the frontend |

```toml
[dependencies]
tauri = { version = "2", features = [] }
duckdb = "1"
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
zip = "2"
serde_json = "1"
```

### Frontend (package.json)

| Package | Purpose |
|---|---|
| `react` + `react-dom` | UI framework |
| `@tauri-apps/api` | `invoke()`, file dialogs, shell commands |
| `tailwindcss` | Utility-first CSS |
| `@radix-ui/*` / `shadcn/ui` | Accessible, styled UI components |
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
If you're making many small Rust changes, consider testing the logic in a standalone
`main.rs` test first, then integrating. Avoid changing `Cargo.toml` unnecessarily as
adding dependencies triggers the slowest recompiles.

### Frontend

**File paths on Windows**
Windows paths use backslashes. When passing a file path from the frontend to Rust,
pass it as-is from the Tauri file dialog — don't manipulate it in JavaScript.
The Tauri dialog APIs return properly formatted paths.

**invoke() not awaited**
`invoke()` returns a Promise. Forgetting `await` causes silent failures where the
UI moves on before the backend has responded.

**State update timing**
React state updates are asynchronous. If you set state and immediately read it in
the same function, you'll get the old value. Use the updated value from the setter
callback or restructure the flow.

### DuckDB / Profiles

**Column names with special characters**
DuckDB requires column names with spaces, #, /, or other special characters to be
wrapped in double quotes in SQL: `"Item #"` not `Item #`.

**Excel files with merged cells or header rows above row 1**
`read_xlsx()` assumes row 1 is the header. If the vendor file has a title row above
the headers, add `OFFSET 1` or handle it in a CTE within the SQL.

**{{input_file}} path with backslashes**
When substituting the file path into SQL on Windows, backslashes in paths may need
to be escaped or replaced with forward slashes. DuckDB accepts forward slashes on
Windows: replace `\` with `/` in `db.rs` before injecting into the SQL string.

---

## 15. Glossary

| Term | Definition |
|---|---|
| **Tauri** | Framework for building desktop apps with a web frontend and Rust backend |
| **Renderer process** | The web page / React app running inside the WebView |
| **Main process** | The Rust binary that owns the window and system access |
| **IPC** | Inter-process communication — how the frontend and backend talk |
| **invoke()** | TypeScript function that calls a Rust command and returns a Promise |
| **#[tauri::command]** | Rust attribute that marks a function as callable from the frontend |
| **WebView2** | Windows' built-in web renderer (like a lightweight browser engine) — used by Tauri |
| **DuckDB** | Embedded SQL database that reads files directly and runs analytical queries |
| **Profile bundle** | A `.import` zip containing `structure.yaml`, `instructions.md`, `sql/`, and optional `fixtures/` and `assets/` |
| **Import profile** | A profile with `inputs` / `outputs` / `steps` — uploads in, CSVs out |
| **Report profile** | A profile with `kind: report` — parameters in, live RE data on screen |
| **Step** | One entry in an import profile's `steps:` list; its `type` picks the UI component |
| **Placeholder** | A `{{…}}` token in profile SQL or YAML the runtime substitutes (`{{input:X}}`, `{{output:X}}`, `{{query:X}}`, `{{sync:X}}`, `{{codetable:X}}`, `{{param:X}}`) |
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
