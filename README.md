# Import Tool

A lightweight Windows desktop app that transforms vendor CSV/Excel files into the column format required by a target database's import tool. Transformation logic lives in external profile bundles — no recompiling needed to add a new vendor.

**Stack:** Tauri (Rust) · React · TypeScript · Tailwind · DuckDB

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
npm run tauri dev
```

This compiles the Rust backend, starts the Vite dev server, and opens a live window.

- **Frontend changes** (`.tsx`, `.css`) hot-reload instantly.
- **Backend changes** (`.rs`) trigger a Rust recompile (5–30 seconds).

---

## Project Structure

```
import-tool/
├── src/                        React frontend
│   ├── App.tsx
│   └── components/
│       ├── ProfilePicker.tsx
│       ├── FileDropZone.tsx
│       └── ResultsPanel.tsx
├── src-tauri/                  Rust backend
│   └── src/
│       ├── main.rs
│       ├── commands.rs
│       ├── profile.rs
│       ├── db.rs
│       └── errors.rs
└── profiles/                   Vendor profile bundles (not compiled in)
    └── vendor_name.zip
        ├── profile.yaml
        ├── transform.sql
        └── README.md
```

---

## Profiles

A profile is a `.zip` bundle that teaches the app how to handle one vendor's file format. Drop a new zip into `profiles/` and relaunch — no code changes needed.

**profile.yaml**
```yaml
name: "Vendor A — Inventory Feed"
description: "Maps Vendor A's weekly export to DB import format"
accepts: [".csv", ".xlsx"]
expected_columns: ["Item #", "Description", "Unit Cost"]
output_filename_prefix: "vendor_a_import"
```

**transform.sql** — a DuckDB query with `{{input_file}}` as the path placeholder:
```sql
SELECT
    "Item #"          AS item_id,
    TRIM("Description") AS item_name,
    "Unit Cost"       AS unit_cost
FROM read_csv_auto('{{input_file}}')
WHERE "Item #" IS NOT NULL;
```

---

## Build

```bash
npm run tauri build
```

Output: `src-tauri/target/release/bundle/` — contains an `.msi` installer and a standalone `.exe`.

Ship the binary alongside the `profiles/` folder.
