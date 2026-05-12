# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Windows desktop application built with **Tauri 2** (Rust backend + React frontend) that
transforms vendor-supplied CSV and Excel files into a target database's import format.
Transformation logic lives in self-contained **profile bundles** (`.import` files, which are
zip archives containing YAML, SQL, and Markdown) — no recompiling required to add a new vendor.

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
import-tool/
├── src/                          # FRONTEND — React app
│   ├── main.tsx                  # React entry point
│   ├── types.ts                  # Shared TypeScript types — mirrors Rust structs exactly
│   ├── App.tsx                   # Root component — all shared state + all invoke() calls
│   ├── lib/
│   │   └── utils.ts              # cn() helper for Tailwind class composition
│   └── components/
│       ├── Titlebar.tsx          # Top window chrome
│       ├── Sidebar.tsx           # Profile picker + step progress list
│       ├── MainPanel.tsx         # Renders one section per step from the loaded profile
│       ├── steps/
│       │   ├── StepSelectFiles.tsx   # file_input step rendering
│       │   ├── StepGenerateFile.tsx  # sql_transform step rendering
│       │   └── StepImport.tsx        # manual_instruction step rendering
│       └── ui/                   # Shadcn primitives (button, popover, command)
│
├── src-tauri/                    # BACKEND — Rust binary
│   ├── Cargo.toml                # Rust dependencies
│   └── src/
│       ├── main.rs               # Entry point — registers all commands
│       ├── commands.rs           # #[tauri::command] functions (thin API layer)
│       ├── profile.rs            # Profile bundle loading (unzip, parse YAML + SQL + Markdown)
│       ├── db.rs                 # DuckDB validation and SQL transforms — writes output CSV
│       └── errors.rs             # Shared AppError enum
│
└── profiles/                     # PROFILE BUNDLES — not compiled in, ship alongside exe
    ├── src/<name>/               # Source folder per profile — structure.yaml, instructions.md, sql/, optional test-files/
    ├── build.sh                  # Repacks each src/<name>/ into <name>.import
    └── <name>.import             # zip archive consumed by the running app
```

---

## The Five Backend Commands

Every command must be registered in `main.rs` inside `generate_handler![]` or `invoke()` fails silently.

| Command | Called from | Args | Returns |
|---|---|---|---|
| `list_profiles` | `App.tsx` on mount | _(none)_ — derives path from `CARGO_MANIFEST_DIR` (dev) or `current_exe` (release) | `ProfileSummary[]` |
| `load_profile` | `App.tsx` on profile select | `zipPath` | `LoadedProfile` |
| `validate_file` | `App.tsx` on validate click | `filePath`, `inputLabel`, `zipPath` | `ValidationResult` |
| `run_profile` | `App.tsx` on generate click | `filePaths` (map of input label → file path), `sqlFile`, `zipPath`, `outputLabels` | `TransformResult` |
| `save_output` | `App.tsx` on download click | `srcPath`, `destPath` | `void` |

`zipPath` for `validate_file` and `run_profile` is actually the extracted temp dir
from `loadedProfile.temp_dir`, not the original `.import` zip. Naming kept for
backwards compatibility — the backend re-reads `structure.yaml` and SQL files from
that directory.

---

## Profile Bundle Format

Each profile is a **`.import` file** (a renamed zip) containing:

- **`structure.yaml`** — metadata (id, name, version, inputs, outputs, steps)
- **`instructions.md`** — step-by-step markdown, split into sections by `<!-- label: StepLabel -->` HTML comments; content before the first anchor is stored as `_header`
- **`sql/`** — folder of `.sql` files; each `sql_transform` step names one file via `step.sql`

`structure.yaml` schema:
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

Step types supported: `file_input`, `sql_transform`, `manual_instruction`.
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

Column names with spaces, `#`, `/` etc. must be double-quoted in SQL: `"Item #"`.

---

## Frontend State Architecture

`App.tsx` owns all shared state (`files`, `generations`, `selectedProfile`) and
is the **only** place that calls `invoke()`. Handlers (`handleFileSelect`,
`handleValidate`, `handleGenerate`, `handleDownload`, `handleSelectProfile`)
live in `App.tsx` and are passed down as props.

Render hierarchy:
- `App.tsx` → `Titlebar` + `Sidebar` + `MainPanel`
- `MainPanel` iterates `loadedProfile.structure.steps` and dispatches each to
  the matching step component:
  - `steps/StepSelectFiles.tsx` for `file_input` — one row per declared input,
    each with Upload + filename pill + Validate button. Inline error table
    when validation fails.
  - `steps/StepGenerateFile.tsx` for `sql_transform` — pipeline diagram
    (inputs → DB → outputs) + Generate button + progress bar + per-output
    Download buttons. Renders one pipeline per transform when the step has a
    `transforms:` array.
  - `steps/StepImport.tsx` for `manual_instruction` — renders the markdown
    body with image assets resolved against `loadedProfile.temp_dir`.

The `loadedProfile.temp_dir` is threaded through `MainPanel` to each step
component and passed back to `validate_file` and `run_profile` so Rust can
re-read validation rules and SQL.

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

---

## What NOT to Do

- Do not modify `build.rs` — it is the Tauri build script
- Do not put file I/O or DuckDB logic in React components
- Do not put UI state management in Rust commands
- Do not manipulate Windows file paths in JavaScript
- Do not add a Cargo dependency without checking if it's already available (slow recompile)
