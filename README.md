# Multitool

A lightweight app — shipping as both a **Windows desktop build** and a
**self-hosted web server** from one codebase — with two workspaces:

- **Imports** — transform vendor CSV/Excel files into the column format a target
  database's import tool expects, with optional live lookups and write-backs
  against the RE NXT (Blackbaud SKY) API.
- **Reports** — parameterized, on-screen reports pulled live from the RE API.

All of that logic lives in external **profile bundles**, not in the binary. Add a
vendor or a report by shipping a `.import` file — no recompiling.

**Stack:** Rust (Tauri 2 desktop shell · Axum web shell) · React · TypeScript ·
Tailwind · DuckDB

The engine is a framework-neutral Rust crate (`crates/core`) wrapped by two thin
shells. One React app serves both: it calls the backend through
`src/lib/api.ts`, which uses Tauri IPC inside the desktop webview and HTTP in a
browser. Profiles, SQL, and fixtures behave identically on both.

---

## Documentation map

| Document | Read it when |
|---|---|
| **[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)** | **You are building a new import or report profile. Start here.** |
| [STEP_TYPES.md](STEP_TYPES.md) | You need the exact YAML fields and UI behavior of one import step type |
| [REPORT_PROFILES.md](REPORT_PROFILES.md) | You need the exact contract for a report profile's five sections |
| **[SERVER.md](SERVER.md)** | **You are deploying, configuring, or operating the web server** |
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
| [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) | Windows web renderer (desktop build, for testing on Windows) | Ships with Windows 11; auto-installs on Windows 10 |
| [Docker](https://docs.docker.com/get-docker/) | Optional — the simplest way to run the web server | Docker Desktop |

On **macOS** (dev only — the desktop build targets Windows): WebView2 is not needed; Tauri uses the system WebKit renderer for local development. The web server builds and runs natively on macOS and Linux.

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

**Desktop**

```bash
npm run tauri dev            # compiles Rust, starts Vite, opens the window
npm run dev                  # frontend only, no Tauri window
RE_NXT_MOCK=1 npm run tauri dev   # force fixture ("mock") mode for RE calls
```

**Web server**

```bash
npm run build                                    # build the SPA into dist/
cargo run -p multitool-server                    # serve it on :8080
# or, everything in a container:
docker compose up --build
```

With no RE credentials configured the server starts in mock mode — the whole
app is clickable without touching Blackbaud. → **[SERVER.md](SERVER.md)**

**Both**

```bash
npx tsc --noEmit             # type-check the frontend
cargo test --workspace       # Rust tests (engine + shells)
cargo build --workspace      # build everything
./profiles/build.sh          # verify + repack every profile bundle
```

- **Frontend changes** (`.tsx`, `.css`) hot-reload instantly.
- **Backend changes** (`.rs`) trigger a Rust recompile (5–30 seconds). Changes
  under `crates/core/` rebuild both shells.
- **Built-in profile changes** need `./profiles/build.sh` *and* a restart — the
  bundles are embedded in the binary at compile time.

---

## Project Structure

A Cargo workspace: the engine in `crates/core`, one thin shell per target.

```
tauri-import/
├── Cargo.toml                      Workspace root (crates/core, crates/server, src-tauri)
├── src/                            React frontend — one app, both targets
│   ├── App.tsx                     Shared state and backend calls
│   ├── types.ts                    Mirrors the Rust structs exactly
│   ├── lib/api.ts                  THE transport module — invoke() vs fetch()
│   └── components/
│       ├── imports/                Imports workspace — Sidebar, MainPanel, steps/
│       ├── reports/                Reports workspace — inputs + viz/ registry
│       ├── data-request/           Data Requests workspace
│       ├── settings/               Settings, including the in-app profile editor
│       ├── shared/                 CodeMirror editor, notice block, panel
│       └── ui/                     Shadcn primitives
├── crates/
│   ├── core/src/                   THE ENGINE — no Tauri, no HTTP
│   │   ├── api.rs                  Every operation as a plain fn; both shells wrap these
│   │   ├── workspace.rs            Session dirs, opaque artifact ids, reaper
│   │   ├── creds.rs                RE NXT connection model, storage, token refresh
│   │   ├── profile.rs              Bundle load/save/duplicate; YAML structs
│   │   ├── validate.rs             Profile linting + file scaffolding
│   │   ├── db.rs                   DuckDB: file validation, transforms, result sets
│   │   ├── re_calls.rs             The one place a SKY API call is executed
│   │   ├── code_tables.rs          Code table pulls and code_table_sync writes
│   │   ├── query_step.rs           The re_query step runner
│   │   ├── user_input.rs           The user_input step runner (forms → {{form:…}})
│   │   ├── report.rs               The report pipeline
│   │   └── errors.rs               Shared AppError enum
│   └── server/src/                 WEB SHELL — Axum
│       ├── main.rs                 Routes, uploads, downloads, OAuth callback, SPA
│       └── creds.rs                Server-wide RE connection store
├── src-tauri/src/                  DESKTOP SHELL — Tauri
│   ├── main.rs                     Entry point — registers every command
│   ├── commands.rs                 #[tauri::command] wrappers over core::api
│   └── sky_auth.rs                 Loopback OAuth listener (desktop-only half)
├── Dockerfile · docker-compose.yml Web server image
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
(`BUILTIN_PROFILES` in `crates/core/src/profile.rs`); user profiles are
`.import` files in the app data directory —
`~/Library/Application Support/com.navin.tauri-import/profiles/` on macOS,
`%APPDATA%\com.navin.tauri-import\profiles\` on Windows, and `DATA_DIR/profiles/`
on the server. Both appear in the sidebar; user profiles can also be created
and edited inside the app under **Settings → Imports**.

→ Full authoring guide: **[PROFILE_AUTHORING.md](PROFILE_AUTHORING.md)**

---

## Build

Run `./profiles/build.sh` first either way — built-ins are compiled into the
binary.

**Desktop (Windows installer)**

```bash
./profiles/build.sh
npm run tauri build
```

Output: `src-tauri/target/release/bundle/` — an `.msi` installer and a
standalone `.exe`.

**Web server (container)**

```bash
./profiles/build.sh
docker build -t multitool-server .
```

The image builds the SPA and the server and runs with one mounted volume at
`/data`. CI builds both from the same tag (`.github/workflows/build.yml`).

→ Deployment and configuration: **[SERVER.md](SERVER.md)**
