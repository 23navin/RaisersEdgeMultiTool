# Multitool

A lightweight Windows desktop app with two workspaces:

- **Imports** — transform vendor CSV/Excel files into the column format a target
  database's import tool expects, with optional live lookups and write-backs
  against the RE NXT (Blackbaud SKY) API.
- **Reports** — parameterized, on-screen reports pulled live from the RE API.

All of that logic lives in external **profile bundles**, not in the binary. Add a
vendor or a report by shipping a `.import` file — no recompiling.

**Stack:** Tauri 2 (Rust) · React · TypeScript · Tailwind · DuckDB

---

## Documentation map

| Document | Read it when |
|---|---|
| **[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)** | **You are building a new import or report profile. Start here.** |
| [STEP_TYPES.md](STEP_TYPES.md) | You need the exact YAML fields and UI behavior of one import step type |
| [REPORT_PROFILES.md](REPORT_PROFILES.md) | You need the exact contract for a report profile's five sections |
| [import_tool_reference.md](import_tool_reference.md) | You are working on the app itself and want the architecture tour |
| [CLAUDE.md](CLAUDE.md) | You are an agent or new contributor changing app code |
| [ui-implementation.md](ui-implementation.md) | Historical — the original UI build spec |
| `API reference/` | SKY OpenAPI specs (`query.yaml`, `codetable.yaml`, …) plus Blackbaud's query-synchronize best practices |

---

## Prerequisites

| Tool | Purpose | Install |
|---|---|---|
| [Rust](https://rustup.rs/) | Compiles the backend binary | `rustup-init` |
| [Node.js](https://nodejs.org/) (v18+) | Frontend toolchain | Download or `nvm` |
| [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) | Windows web renderer (for testing on Windows) | Ships with Windows 11; auto-installs on Windows 10 |

On **macOS** (dev only — app targets Windows): WebView2 is not needed; Tauri uses the system WebKit renderer for local development.

---

## Environment Setup

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup update

# 2. Install the Tauri CLI
cargo install tauri-cli

# 3. Install frontend dependencies
npm install
```

---

## Development

```bash
npm run tauri dev            # compiles Rust, starts Vite, opens the window
npm run dev                  # frontend only, no Tauri window
npx tsc --noEmit             # type-check
./profiles/build.sh          # verify + repack every profile bundle
RE_NXT_MOCK=1 npm run tauri dev   # force fixture ("mock") mode for RE calls
```

- **Frontend changes** (`.tsx`, `.css`) hot-reload instantly.
- **Backend changes** (`.rs`) trigger a Rust recompile (5–30 seconds).
- **Built-in profile changes** need `./profiles/build.sh` *and* a restart — the
  bundles are embedded in the binary at compile time.

---

## Project Structure

```
tauri-import/
├── src/                            React frontend
│   ├── App.tsx                     Shared state; the only place invoke() is called
│   ├── types.ts                    Mirrors the Rust structs exactly
│   └── components/
│       ├── imports/                Imports workspace — Sidebar, MainPanel, steps/
│       ├── reports/                Reports workspace — inputs + viz/ registry
│       ├── data-request/           Data Requests workspace
│       ├── settings/               Settings, including the in-app profile editor
│       ├── shared/                 CodeMirror editor, notice block, panel
│       └── ui/                     Shadcn primitives
├── src-tauri/                      Rust backend
│   └── src/
│       ├── main.rs                 Entry point — registers every command
│       ├── commands.rs             #[tauri::command] API layer
│       ├── profile.rs              Bundle load/save/duplicate; YAML structs
│       ├── validate.rs             Profile linting + file scaffolding
│       ├── db.rs                   DuckDB: file validation, transforms, result sets
│       ├── re_calls.rs             The one place a SKY API call is executed
│       ├── code_tables.rs          Code table pulls and code_table_sync writes
│       ├── query_step.rs           The re_query step runner
│       ├── report.rs               The report pipeline
│       ├── sky_auth.rs             RE NXT connection + credentials
│       └── errors.rs               Shared AppError enum
└── profiles/                       Profile bundles
    ├── src/<name>/                 Source: structure.yaml, instructions.md, sql/, fixtures/
    ├── build.sh                    Verifies each source folder, packs it to <name>.import
    └── <name>.import               The zip the app actually consumes
```

---

## Profiles

A profile is a zip (extension `.import`) holding `structure.yaml`, an optional
`instructions.md`, a `sql/` folder, and optional `fixtures/` and `assets/`.
There are two kinds — **import** (uploads → steps → output CSVs) and **report**
(`kind: report`; parameters → RE queries → SQL → visualizations).

Minimal import profile:

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
      - { label: "Item #", required: true, type: number, digits: 6 }

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

`sql/primary_transform.sql` — a DuckDB query that names its input through a
placeholder:

```sql
SELECT
    "Item #"            AS item_id,
    TRIM("Description") AS item_name,
    "Unit Cost"         AS unit_cost
FROM read_csv_auto('{{input:Classification}}')
WHERE "Item #" IS NOT NULL;
```

**Where profiles come from:** built-ins are embedded in the binary
(`BUILTIN_PROFILES` in `src-tauri/src/profile.rs`); user profiles are `.import`
files in the app data directory
(`~/Library/Application Support/com.navin.tauri-import/profiles/` on macOS,
`%APPDATA%\com.navin.tauri-import\profiles\` on Windows). Both appear in the
sidebar; user profiles can also be created and edited inside the app under
**Settings → Imports**.

→ Full authoring guide: **[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)**

---

## Build

```bash
./profiles/build.sh      # first — built-ins are compiled into the binary
npm run tauri build
```

Output: `src-tauri/target/release/bundle/` — contains an `.msi` installer and a
standalone `.exe`.
