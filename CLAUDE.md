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
│   ├── App.tsx                   # Root component — all shared state + all invoke() calls except step actions
│   └── components/
│       ├── ProfilePicker.tsx     # Profile dropdown
│       ├── StepPanel.tsx         # Active step UI (file_input / validation / sql_transform / manual_instruction)
│       └── InstructionsPanel.tsx # Right sidebar — renders step markdown instructions
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
    └── vendor_name.import        # zip archive containing structure.yaml, instructions.md, sql/
```

---

## The Four Backend Commands

Every command must be registered in `main.rs` inside `generate_handler![]` or `invoke()` fails silently.

| Command | Called from | Args | Returns |
|---|---|---|---|
| `list_profiles` | `App.tsx` on mount | `profilesDir` | `ProfileSummary[]` |
| `load_profile` | `App.tsx` on profile select | `zipPath` | `LoadedProfile` |
| `validate_file` | `StepPanel.tsx` on validation step | `filePath`, `profileId`, `inputLabel`, `zipPath` | `ValidationResult` |
| `run_profile` | `StepPanel.tsx` on sql_transform step | `filePath`, `sqlFile`, `zipPath`, `outputLabel` | `TransformResult` |

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
    input: ["Classification"]
  - label: Validate
    type: validation
    input:
      - label: Classification
        validate: true
  - label: Transform
    type: sql_transform
    input: ["Classification"]
    sql: primary_transform.sql
    output: ["Import File"]
```

SQL files use `{{input_file}}` as a placeholder — replaced at runtime. DuckDB reads files directly:
- `read_csv_auto('{{input_file}}')` for CSV
- `read_xlsx('{{input_file}}')` for Excel

Column names with spaces, `#`, `/` etc. must be double-quoted in SQL: `"Item #"`.

---

## Frontend State Architecture

`App.tsx` owns all shared state (`AppState` from `types.ts`) and is the **only** place that calls
`list_profiles` and `load_profile`. Child components receive data and callbacks as props.

`StepPanel.tsx` is the exception — it calls `validate_file` and `run_profile` directly, then
reports results up via `onStepComplete`, `onStepError`, and `onOutputReady` callbacks.

Step types and what `StepPanel` renders for each:
- `file_input` — native file picker (Tauri dialog plugin), per-input attach buttons
- `validation` — calls `validate_file`, shows column-level errors
- `sql_transform` — calls `run_profile`, shows output file path and row count
- `manual_instruction` — no app action, just a "Mark Complete" button

The `loadedProfile.temp_dir` (where the zip was extracted) is threaded through to `StepPanel`
and passed back to `validate_file` and `run_profile` so Rust can re-read validation rules and SQL.

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
- **`validate_file` uses `zipPath` not a temp dir path** → The backend re-extracts from the original zip path

---

## What NOT to Do

- Do not modify `build.rs` — it is the Tauri build script
- Do not put file I/O or DuckDB logic in React components
- Do not put UI state management in Rust commands
- Do not manipulate Windows file paths in JavaScript
- Do not add a Cargo dependency without checking if it's already available (slow recompile)
