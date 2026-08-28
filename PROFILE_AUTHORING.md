# Profile Authoring Guide

**Start here if you are building a new import or report profile.**

This document is the front door for profile authors. It assumes you know SQL and
YAML, and nothing else about this repo. You do **not** need to read the Rust or
React source, and you do **not** need to recompile the app to ship a profile.

| If you want to… | Read |
|---|---|
| Build a profile end to end | **this file** |
| Look up one import step type's exact fields | [STEP_TYPES.md](STEP_TYPES.md) |
| Look up the report sections' exact fields | [REPORT_PROFILES.md](REPORT_PROFILES.md) |
| Understand the app's architecture | [import_tool_reference.md](import_tool_reference.md) |
| Work on the app itself (not a profile) | [CLAUDE.md](CLAUDE.md) |

---

## Contents

1. [The 60-second model](#1-the-60-second-model)
2. [Which kind of profile do I need?](#2-which-kind-of-profile-do-i-need)
3. [Two ways to author a profile](#3-two-ways-to-author-a-profile)
4. [Bundle anatomy](#4-bundle-anatomy)
5. [Walkthrough — build an import profile](#5-walkthrough--build-an-import-profile)
6. [Walkthrough — build a report profile](#6-walkthrough--build-a-report-profile)
7. [Building-block catalog](#7-building-block-catalog)
8. [Placeholder reference](#8-placeholder-reference)
9. [The wiring contract — named outputs, declared inputs](#9-the-wiring-contract--named-outputs-declared-inputs)
10. [Working offline — mock mode and fixtures](#10-working-offline--mock-mode-and-fixtures)
11. [Verify and ship](#11-verify-and-ship)
12. [Copy from an example](#12-copy-from-an-example)
13. [Troubleshooting](#13-troubleshooting)
14. [Author's checklists](#14-authors-checklists)

---

## 1. The 60-second model

A **profile** is a zip file with the extension `.import`. It holds YAML, SQL,
Markdown, and optional JSON fixtures. The app reads it at runtime; the app's
binary never changes when you add or edit one.

There are two kinds, distinguished by one top-level YAML key:

```yaml
kind: report      # omit the key (or set `import`) for an import profile
```

```
IMPORT PROFILE                              REPORT PROFILE
user uploads files                          user sets parameters
        │                                           │
        ▼                                           ▼
  steps run in order                        queries hit the RE API
  (upload → SQL → RE query →                        │
   code-table sync → manual)                        ▼
        │                                   transforms (DuckDB SQL)
        ▼                                           │
  output CSVs the user downloads                    ▼
                                            visualizations on screen
                                            + optional write-back actions
```

Everything in between is DuckDB SQL. DuckDB reads the user's CSV/XLSX files and
the JSON the app pulls from the RE (Blackbaud SKY) API **directly from disk** —
there is no loading step, no schema to declare, no database to manage. Your SQL
just names a file through a `{{placeholder}}` and selects from it.

The three things you write, every time:

1. **`structure.yaml`** — what the profile takes in, what it produces, and the
   ordered list of things that happen.
2. **`sql/*.sql`** — the actual work.
3. **`instructions.md`** — what the user reads on screen, split per step.

---

## 2. Which kind of profile do I need?

| Question | Import profile | Report profile |
|---|---|---|
| Where does the data come from? | A file the user uploads | The RE API |
| What does the user get? | A CSV to download and import elsewhere | Tables/charts on screen |
| Does it write back to RE? | Only via a `code_table_sync` step | Only via an `actions` button |
| Is there a step-by-step workflow? | Yes — an ordered checklist | No — one Refresh button |
| Which tab is it in? | Imports | Reports |

Both kinds can pull RE **code tables**, and both run their SQL in the same
DuckDB engine. If you need "upload a file, ask RE about it, produce a CSV",
that's an **import** profile with an `re_query` step — not a report.

---

## 3. Two ways to author a profile

### Route A — In the app (fastest loop, best for user profiles)

Open **Settings → Imports**. This is a full editor over the profile bundle:

| Action | What it does |
|---|---|
| **New profile** (sidebar) | Creates an empty user bundle (`structure.yaml` + `instructions.md`) with a unique id |
| **Duplicate** / **Duplicate to edit** | Copies a profile into your user profiles. Built-ins open read-only, so this is how you start from one |
| File tree + editor | Edits `structure.yaml`, `instructions.md`, and every `sql/*.sql` with syntax highlighting and cross-file anchor navigation |
| **Validate** | Runs the structural checks and lists each issue with the exact line to jump to |
| **Scaffold N missing items** (issues panel) | Generates stub `.sql` files for every step that names one it doesn't have, projecting the input's declared columns. Appears only when the report has fixable issues; press Save to persist |
| **Save** | Repacks the `.import` zip in place (binary assets are preserved byte-for-byte). Ctrl/Cmd-S validates first and saves only on a clean report |
| **Delete** | Removes a user profile — two-click to confirm; refuses built-ins |

User profiles live in the app data directory:

- macOS: `~/Library/Application Support/com.navin.tauri-import/profiles/`
- Windows: `%APPDATA%\com.navin.tauri-import\profiles\`

Anything you drop there as a `.import` file shows up the next time the app lists
profiles (relaunching the window is enough) — no build step, no Rust recompile.

> The in-app validator currently implements the **import**-profile rules. A
> report bundle will load and run, but its report-specific sections are only
> deeply checked by Route B's build script. Author reports through Route B.

### Route B — In the repo (source of truth, required for built-ins)

Each profile has a source folder under `profiles/src/<name>/`, and
`profiles/build.sh` verifies and repacks every one of them:

```bash
./profiles/build.sh          # verifies each src/<name>/, then zips it to <name>.import
```

The script refuses to pack a profile that fails verification, so a green run
means the bundle is structurally sound.

If your profile is a **built-in** (compiled into the binary via `include_bytes!`
in `src-tauri/src/profile.rs`), you must run `build.sh` *before* the Rust build
— the `.import` file has to exist on disk at compile time — and then restart
`npm run tauri dev` to pick it up.

| | Route A (in-app) | Route B (repo) |
|---|---|---|
| Feedback loop | Instant | Re-run `build.sh` |
| Validation | Import rules, inline, clickable | Import **and** report rules, in the terminal |
| Version-controlled | No (lives in app data) | Yes |
| Can be a built-in | No | Yes |
| Sample input files | Not stored | `test-files/` (excluded from the zip) |

**Recommended flow:** draft in the repo (Route B) so it is reviewable and
diffable, and use Route A for quick edits while testing against a real file.

---

## 4. Bundle anatomy

```
<name>.import                 # a zip; the source lives at profiles/src/<name>/
├── structure.yaml            # REQUIRED — the whole contract
├── instructions.md           # optional  — on-screen prose, split per step
├── sql/                      # optional  — every .sql file the YAML names
│   ├── primary_transform.sql
│   └── notices/
│       └── unknown_categories.sql
├── fixtures/                 # optional  — canned RE responses for mock mode
│   ├── <query output>.json           # report queries
│   ├── queries/<query_output>.json   # import re_query steps
│   └── codetables/<output>.json      # code_tables (both kinds)
├── assets/                   # optional  — images used by instructions.md
│   └── import_profile.png
└── test-files/               # repo only — sample inputs, NOT packed into the zip
    └── sample_constituents.csv
```

`structure.yaml` always opens with the same five keys:

```yaml
id: vendor_a                 # unique-ish slug; the filename is what's truly unique
name: "Vendor A Import"      # shown in the sidebar
version: "1.0"
min_app_version: "0.1.0"     # required key; not enforced at runtime today
kind: report                 # omit entirely for an import profile
```

All four of `id`, `name`, `version`, and `min_app_version` are required — a
bundle missing any of them fails verification. The sidebar keys on the bundle's
path, not `id`, so a built-in and a user profile may share an id.

Then, by kind:

| Import profile | Report profile |
|---|---|
| `inputs:` — files the user uploads | `parameters:` — form controls |
| `outputs:` — files the profile produces | `queries:` — RE API calls |
| `steps:` — the ordered workflow | `transforms:` — DuckDB SQL |
| | `visualizations:` — what renders |
| | `actions:` — write-back buttons |
| `code_tables:` — optional, shared by both kinds | `code_tables:` — same section |

### `instructions.md` — the anchor convention

One Markdown file, split into per-step sections by HTML comments:

```markdown
# Vendor A Import

Short description of the whole profile. Everything above the first anchor is
the profile header shown at the top of the panel.

<!-- label: AddSourceFiles -->
## Select Source Files

Upload the un-edited `inventory.csv` the vendor emailed.

<!-- label: CreateImportFile -->
## Generate Import File

Produces the file you'll feed to the database's bulk importer.
```

- The anchor's text must match a step's `label` exactly (for reports, a
  `visualizations[].id`).
- The `##` heading becomes the on-screen step title; without one, the YAML
  `label` is used.
- `![alt](assets/pic.png)` resolves against the extracted bundle and renders in
  `manual_instruction` steps.

---

## 5. Walkthrough — build an import profile

The goal: a vendor sends `constituents.csv`; we validate it, produce an import
CSV, and tell the user what to do with it.

### Step 1 — Create the folder

```bash
mkdir -p profiles/src/vendor_a/sql
```

Or copy the closest example wholesale — see [§12](#12-copy-from-an-example).

### Step 2 — Declare the inputs

An `input` is a file the user attaches. Its `validation` list is what the
**Validate** button checks *before* any SQL runs.

```yaml
inputs:
  - label: Constituents        # the name you'll use everywhere else
    type: csv                  # csv | xlsx
    required: true
    validation:
      - label: constituent_id  # the column header in the user's file
        required: true         # column must exist and have no blank cells
        type: number           # string | number
        digits: 5              # exact character length (number columns only)
      - label: last_name
        required: true
        type: string
      - label: constituent_code
        required: false
        type: string
        value: ["Alpha", "Beta", "Gamma"]   # controlled vocabulary
```

What each rule actually does:

| Field | Effect when the file is validated |
|---|---|
| `label` | Matched against the file's headers — exact match first, then a case- and punctuation-insensitive match (a fuzzy hit still validates, and reports a "Column name mismatches" notice so you can tighten the profile) |
| `required: true` | Errors if the column is absent, and errors with a count if any cell is null or blank |
| `type: number` | Errors with a count of cells that aren't numeric |
| `type: string` | No check of its own — `required` and `value` still apply |
| `digits: N` | Errors with a count of cells whose text length isn't exactly `N` |
| `value: [...]` | Errors with a count of non-null cells outside the list |

Errors are reported **per rule with a count**, not per offending row.

### Step 3 — Declare the outputs

```yaml
outputs:
  - label: Import_File
    type: csv
```

Every output is written as a timestamped CSV in the temp directory
(`import_file_20260828_141530.csv`) and exposed as a **Download** button; the
user chooses the destination. `type:` is documentation — the writer always emits
CSV.

### Step 4 — Lay out the steps

Steps render top to bottom and the user works through them in order. Five types
exist:

| `type` | What the user sees | Full reference |
|---|---|---|
| `file_input` | Upload + Validate rows | [STEP_TYPES.md](STEP_TYPES.md#step-type-file_input) |
| `sql_transform` | Pipeline diagram + Generate + Download | [STEP_TYPES.md](STEP_TYPES.md#step-type-sql_transform) |
| `re_query` | Run Query — pulls rows from RE mid-pipeline | [STEP_TYPES.md](STEP_TYPES.md#step-type-re_query) |
| `code_table_sync` | Add/Update/Delete entries — writes to an RE code table | [STEP_TYPES.md](STEP_TYPES.md#step-type-code_table_sync) |
| `manual_instruction` | Prose only, no action | [STEP_TYPES.md](STEP_TYPES.md#step-type-manual_instruction) |

The minimal profile is upload → transform → instructions:

```yaml
steps:
  - label: AddSourceFiles
    type: file_input
    input:
      - label: Constituents
        validate: true         # shows the Validate button for this row

  - label: CreateImportFile
    type: sql_transform
    input: [Constituents]      # which inputs the SQL may read
    sql: primary_transform.sql # a file under sql/
    output: [Import_File]      # which declared outputs it writes

  - label: Import
    type: manual_instruction
```

### Step 5 — Write the SQL

`sql/primary_transform.sql`. One bare `SELECT` — the runtime wraps it in a
`COPY` to the single declared output:

```sql
SELECT
    c."constituent_id"      AS record_id,
    trim(c."last_name")     AS last_name,
    trim(c."first_name")    AS first_name
FROM read_csv_auto('{{input:Constituents}}') c
WHERE c."constituent_id" IS NOT NULL
ORDER BY c."constituent_id";
```

Rules that bite:

- Quote every column name that has a space, `#`, `/`, or other punctuation:
  `"Item #"`.
- `{{input:Label}}` is the path to that input's file. Use `read_csv_auto(...)`
  for CSV and `read_xlsx(...)` for Excel.
- `{{input_file}}` is a legacy alias for the same thing, valid **only** when the
  transform declares exactly one input. Prefer the labeled form.
- Excel headers must be on row 1; otherwise skip rows with a CTE or `OFFSET`.

To write **more than one file** from one transform, name each output explicitly
and write your own `COPY` statements — the whole file then runs as a batch, so
CTEs, temp tables, and multiple statements are all fair game:

```sql
COPY (SELECT * FROM read_csv_auto('{{input:Orders}}') WHERE "Status" = 'Shipped')
  TO '{{output:Shipped_Orders}}' (HEADER, DELIMITER ',');

COPY (SELECT * FROM read_csv_auto('{{input:Orders}}') WHERE "Status" = 'Pending')
  TO '{{output:Pending_Orders}}' (HEADER, DELIMITER ',');
```

### Step 6 — Add notices for "valid, but someone should look at this"

A notice is a query that runs after the transform succeeds. Zero rows is the
nominal case; any rows render as an amber table under the Generate row. Notices
never fail the step.

```yaml
  - label: CreateImportFile
    type: sql_transform
    input: [Constituents]
    sql: primary_transform.sql
    output: [Import_File]
    notices:
      - label: "Rows without a code"
        sql: notices/unresolved_rows.sql
        description: >
          These rows import with an empty code. Fix them in the vendor file or
          add the codes in RE first.
```

Notice SQL sees the same placeholders as its transform.

### Step 7 — Write the instructions

```markdown
# Vendor A Import

Turns Vendor A's weekly constituent export into an import file.

<!-- label: AddSourceFiles -->
## Select Source Files

Upload the un-edited `constituents.csv` from the vendor portal.

<!-- label: CreateImportFile -->
## Generate Import File

Builds the import file. Check any notices before continuing.

<!-- label: Import -->
## Import into the database

Run the `SimpleImport` profile in the database's bulk import tool against the
file you just downloaded.
```

### Step 8 — Verify and run

```bash
./profiles/build.sh
npm run tauri dev
```

Pick the profile in the Imports sidebar and work through it with a sample file
from `test-files/`.

### Optional — reach into RE

Two building blocks turn a plain file transform into an RE-aware one. Both are
detailed in [STEP_TYPES.md](STEP_TYPES.md); the shape is:

**Read a code table** (no step needed — declare it and use it):

```yaml
code_tables:
  - id: constituent_codes
    name: "Constituent Codes"   # exact RE name — or code_table_id: "43"
    include_inactive: false
    output: ConstituentCodes    # → {{codetable:ConstituentCodes}}
```

```sql
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(trim(c."constituent_code"))
```

**Ask RE a question about the uploaded rows** (`re_query` step): `params_sql`
pulls the ids out of the file, `template` sends them, and the answer lands at
`{{query:Label}}` for any later transform that declares `query_input`.

**Push new code table entries** (`code_table_sync` step): the step's SQL selects
rows using the API's writable column names (`long_description`, …); each row
becomes one create/update/delete. Name `sync_output` and later SQL can read the
ids RE just assigned at `{{sync:Label}}`.

---

## 6. Walkthrough — build a report profile

The goal: a parameterized, on-screen report over live RE data.

### Step 1 — Declare the kind and the parameters

```yaml
id: gift_activity
name: "Gift Activity Report"
kind: report
version: "1.0"
min_app_version: "0.2.0"

parameters:
  - id: gift_dates
    label: "Gift Date Range"
    type: date_range          # date | date_range | select | text | number
    required: true
    default:
      preset: last_30_days    # the only preset implemented; or use from/to
  - id: fund
    label: "Fund"
    type: select
    options: ["Annual", "Capital", "Endowment"]   # required for select
```

Parameters render in the Inputs panel, separate from the visualizations. A
`date_range` exposes `.from` and `.to` to placeholders; every other type is a
scalar.

### Step 2 — Declare the queries

```yaml
queries:
  - id: gift_rows
    ref: re.query.execute       # registry call — hides SKY's execute→poll→download
    template:                   # an ExecuteQueryDefinition (see API reference/query.yaml)
      type_id: 18
      select_fields:
        - { query_field_id: 597,  user_alias: constituent_name }
        - { query_field_id: 8469, user_alias: gift_amount }
        - { query_field_id: 8471, user_alias: gift_date }
      filter_fields:
        - query_field_id: 8471
          compare_type: None
          operator: Between
          filter_values:
            - "{{param:gift_dates.from}}"
            - "{{param:gift_dates.to}}"
      sort_fields:
        - { query_field_id: 8471, sort_order: Descending }
    output: GiftRows            # → {{query:GiftRows}} in transform SQL
```

Three things to internalize:

1. **`template` is the query *definition*, not the whole request.** The backend
   nests it under `query` and fills in `ux_mode: Asynchronous`,
   `output_format: Json`, and `formatting_mode: None`. If you paste a template
   that already has a top-level `query` key, it is sent untouched.
2. **`query_field_id` values are environment-specific.** Discover yours with
   `GET /query/querytypes?product=RE&module=None`, then
   `GET /query/v2/queryfields/root` / `.../node`. Never assume the ids in the
   examples work in your tenant.
3. **`user_alias` is the seam.** It renames the column in the result, so your
   transform SQL selects stable names and never mentions RE field ids.

Available registry calls (`ref:`): `re.query.execute`, `re.query.create`,
`re.codetable.list`, `re.codetable.entries`, `re.codetable.entry.create`,
`re.codetable.entry.update`, `re.codetable.entry.delete`.

### Step 3 — Transform

```yaml
transforms:
  - id: constituent_gifts
    input: [GiftRows]              # query outputs this SQL may read
    sql: constituent_gifts.sql
    output: ConstituentGifts       # an in-memory result set (columns + rows)
```

```sql
SELECT
    "constituent_name"                   AS constituent_name,
    CAST("gift_amount" AS DECIMAL(18,2)) AS gift_amount,
    "gift_date"                          AS gift_date
FROM read_json_auto('{{query:GiftRows}}')
ORDER BY "gift_date" DESC;
```

A report transform's `output` is a single named result set held in memory — no
CSV is written, and nothing is downloaded.

### Step 4 — Visualize

```yaml
visualizations:
  - id: gifts_table
    type: table                    # table | bar | line | pie | kpi
    title: "Constituents & Gifts"
    data: ConstituentGifts         # a transform output
    config:
      sortable: true
      columns:
        - { field: constituent_name, header: "Constituent" }
        - { field: gift_amount, header: "Amount", format: currency }
        - { field: gift_date, header: "Date", format: date }
```

Each `field` must match a column name your SQL selects.

> **Only `table` is implemented today.** `bar`, `line`, `pie`, and `kpi` are
> registered but render as placeholder cards
> (`src/components/reports/viz/index.tsx`). Build reports around `table` until
> the chart components land.

### Step 5 — Optional write-back action

```yaml
actions:
  - id: save_re_query
    label: "Create Query in RE"
    ref: re.query.create
    input: ConstituentGifts        # the result set supplying the ids
    bind:
      name: "Report Gifts {{now}}"
      id_field: gift_id            # which column holds the ids
      type_id: 18                  # environment-specific
      id_query_field_id: 8467      # environment-specific
```

Actions are **not** part of Refresh — they run only when clicked. RE has no
"save this list of ids" endpoint, so the collected ids are posted as a static
query whose single `OneOf` filter carries them; that's why `type_id` and
`id_query_field_id` are required.

### Step 6 — Fixtures, verify, run

Add `fixtures/GiftRows.json` (a JSON array of row objects) so the report runs
offline, then:

```bash
./profiles/build.sh
RE_NXT_MOCK=1 npm run tauri dev
```

The Reports tab shows a `live`/`mock` badge so you always know which path ran.

---

## 7. Building-block catalog

Everything you can declare, in one place. Follow the link for exact field lists.

### Shared by both kinds

| Block | YAML key | What it gives you |
|---|---|---|
| Identity | `id`, `name`, `version`, `min_app_version` | Sidebar entry and metadata |
| Kind | `kind` | `report`, or omitted for import |
| Code tables | `code_tables` | Pulls an RE code table to JSON before any SQL runs → `{{codetable:Label}}` ([details](STEP_TYPES.md#code-tables-no-step-required)) |
| Instructions | `instructions.md` | Per-step prose, images, inline code |
| Fixtures | `fixtures/` | Canned RE responses so the profile runs with no connection |

### Import-only

| Block | YAML key | What it gives you |
|---|---|---|
| Inputs | `inputs` | Upload slots plus column-level validation rules |
| Outputs | `outputs` | Named CSVs with Download buttons |
| Upload step | `steps[].type: file_input` | Upload + Validate rows, one per declared input |
| Transform step | `steps[].type: sql_transform` | DuckDB SQL → output files; supports multiple transforms per step via `transforms:` |
| Notices | `notices` on a transform | Post-run informational tables (never fail the step) |
| Query step | `steps[].type: re_query` | Reads RE mid-pipeline → `{{query:Label}}` |
| Sync step | `steps[].type: code_table_sync` | Writes code table entries → optional `{{sync:Label}}` |
| Instruction step | `steps[].type: manual_instruction` | Prose-only closing step |

### Report-only

| Block | YAML key | What it gives you |
|---|---|---|
| Parameters | `parameters` | `date`, `date_range`, `select`, `text`, `number` controls |
| Queries | `queries` | RE API calls, parameterized → `{{query:Label}}` |
| Transforms | `transforms` | DuckDB SQL → named in-memory result sets |
| Visualizations | `visualizations` | Rendered components bound to a result set |
| Actions | `actions` | On-demand write-backs |

### The engine underneath

- **DuckDB** runs every SQL file. Any DuckDB SQL is legal: CTEs, window
  functions, `CREATE TEMP TABLE`, `read_csv_auto`, `read_xlsx`,
  `read_json_auto`, `read_json(..., columns={...})`.
- **The RE call registry** (`src-tauri/src/re_calls.rs`) owns auth, the async
  job flow, polling, and result normalization. Profiles never see HTTP.
- **`API reference/`** holds the SKY OpenAPI specs (`query.yaml`,
  `codetable.yaml`, `constituent.yaml`, and more) plus
  *query-sychronize best practices.md* on throttling and job queueing. Consult
  `query.yaml` when writing an `ExecuteQueryDefinition`.

---

## 8. Placeholder reference

Every placeholder the runtime substitutes, and where it is legal.

| Placeholder | Resolves to | Legal in | Requires |
|---|---|---|---|
| `{{input:Label}}` | Path of the uploaded file for that input | Import transform SQL, notice SQL, `params_sql`, sync SQL | The label declared in `inputs:` **and** in the step's `input:` |
| `{{input_file}}` | Same, single-input alias | Same | The transform declares exactly one input |
| `{{output:Label}}` | Temp path for that output file | Import transform SQL | The label declared in `outputs:` and the step's `output:` |
| `{{codetable:Label}}` | JSON path of a pulled code table | Any SQL, both kinds | A `code_tables:` entry whose `output` is `Label` |
| `{{query:Label}}` | JSON path of a query result | Import transform SQL (with `query_input`), report transform SQL | An earlier `re_query` step's `query_output`, or a report `queries[].output` |
| `{{sync:Label}}` | JSON path of a sync step's outcome rows | Import transform SQL | An earlier `code_table_sync` step's `sync_output`, declared in `sync_input` |
| `{{rows:col}}` | JSON **array** of that column's values from `params_sql` (deduped, blanks dropped, order kept) | `re_query` step `template` | The value is *exactly* the placeholder, e.g. `filter_values: "{{rows:record_id}}"` |
| `{{value:col}}` | The first row's cell from `params_sql`, substituted inline | `re_query` step `template` | — |
| `{{param:id}}` / `{{param:id.from}}` / `{{param:id.to}}` | A report parameter's value | Report `queries[].template` / `bind` | A matching `parameters[].id` |
| `{{now}}` | Current timestamp | Report `actions[].bind` | — |

Read the JSON ones with `read_json_auto(...)` — **except** `{{sync:Label}}`,
which is legitimately empty when the sync had nothing to push. Auto-detection
has no schema to infer from an empty file, so name the columns explicitly:

```sql
read_json('{{sync:NewCodes}}', columns={'long_description': 'VARCHAR',
                                        'table_entries_id': 'VARCHAR',
                                        'sync_status': 'VARCHAR'})
```

---

## 9. The wiring contract — named outputs, declared inputs

Everything an import step publishes for a later step follows the same pattern.
Learn it once and both families read the same:

```
producer step        names its result       consumer transform      SQL reads
─────────────        ────────────────       ──────────────────      ─────────
re_query          →  query_output: Foo   →  query_input: [Foo]   →  {{query:Foo}}
code_table_sync   →  sync_output:  Bar   →  sync_input:  [Bar]    →  {{sync:Bar}}
```

Three rules the verifiers enforce:

1. **The producer must appear earlier in `steps:`.** There is no topological
   sort — steps run when the user clicks them. A forward reference is rejected
   with *"the … step producing it comes later — reorder the steps"*.
2. **The consumer must declare the dependency.** A `{{query:Foo}}` in SQL whose
   transform never lists `Foo` in `query_input` is an error, not a silent pass.
   The declaration is what makes the dependency visible in the UI and precise
   for invalidation.
3. **Names are unique per family.** Two steps can't publish the same
   `query_output`.

Why declaring matters at runtime: re-running a query clears every transform that
declares it, and re-uploading a file clears the query steps that read it *and*
their downstream transforms. A stale join is worse than a missing one.

The same shape is used by `code_tables:` (`output` → `{{codetable:…}}`), except
code tables are pulled fresh before *every* run, so there is nothing to order and
nothing to declare.

---

## 10. Working offline — mock mode and fixtures

Any profile that touches RE runs in one of two transports:

| Transport | When | Reads |
|---|---|---|
| **Live** | An RE NXT connection exists (Settings → General) and `RE_NXT_MOCK` is unset | The real SKY API |
| **Mock** | No connection, or `RE_NXT_MOCK=1` | `fixtures/` in the bundle |

```bash
RE_NXT_MOCK=1 npm run tauri dev     # force the fixture path
```

Fixture locations, exactly:

| Feature | Fixture path | Shape |
|---|---|---|
| Report query | `fixtures/<queries[].output>.json` | JSON array of row objects |
| Import `re_query` step | `fixtures/queries/<query_output>.json` | JSON array of row objects |
| Code table (either kind) | `fixtures/codetables/<code_tables[].output>.json` | JSON array of `TableEntry` objects |

Mock mode ignores `template` entirely and returns the fixture verbatim, so keep
the fixture's column names identical to your `user_alias` values or your SQL
will bind in live mode and fail in mock (or vice versa).

**Writes in mock mode are stubbed, not sent.** A `code_table_sync` step still
publishes its `sync_output` rows with deterministic `mock-N` ids and
`sync_mode: mock`, so downstream SQL joins identically in both modes. The
live/mock badge sits next to the button *before* the user clicks — that step
changes RE data.

---

## 11. Verify and ship

### Verify

```bash
./profiles/build.sh          # verifies every profile, then packs the passing ones
```

The verifier and the in-app **Validate** button check the same class of things.
Common issue codes and what they mean:

| Code | Meaning |
|---|---|
| `yaml.parse_failed` | `structure.yaml` isn't valid YAML — nothing else can be checked |
| `yaml.unknown_step_type` | `type:` isn't one of the five |
| `yaml.duplicate_step_label` / `_input_` / `_output_label` | Labels must be unique |
| `yaml.input_ref.undeclared` / `yaml.output_ref.undeclared` | A step names a label that isn't in `inputs:` / `outputs:` |
| `yaml.sql_transform.missing_sql_field` / `missing_sql_file` | `sql:` absent, or names a file that isn't in `sql/` |
| `yaml.sql_transform.ambiguous_shape` | Both the step-level shortcut and `transforms:` are present |
| `yaml.validate_without_columns` | `validate: true` on an input with no `validation:` rules |
| `yaml.code_tables.no_table` / `no_output` / `duplicate_output` | A `code_tables:` entry is missing `name`/`code_table_id`, missing `output`, or reuses one |
| `yaml.re_query.no_call` / `no_output` / `missing_params_sql` | An `re_query` step has neither `ref` nor `template`, no `query_output`, or names a missing `params_sql` |
| `yaml.code_table_sync.no_table` / `no_operation` / `bad_operation` | Sync step misconfigured (`operation` must be create/update/delete) |
| `yaml.query_input.unresolved` / `yaml.sync_input.unresolved` | Declared dependency doesn't exist, or its producer comes later |
| `sql.unknown_query` / `sql.unknown_sync` / `sql.unknown_code_table` | SQL uses a `{{…:Label}}` nothing declares |
| `sql.placeholder_input.undeclared` / `placeholder_output.undeclared` | SQL names an input/output the step doesn't declare |
| `sql.input_file_multi` | `{{input_file}}` in a multi-input transform |
| `sql.output_mismatch` | The `{{output:…}}` placeholders don't match the declared outputs |
| `sql.parse_failed` | DuckDB rejected the SQL at compile time |
| `sql.orphan_file` | A file in `sql/` that no step references |
| `sql.code_table_sync.no_long_description` | A `create` sync whose SQL never selects `long_description` |
| `md.step_missing_anchor` / `md.orphan_anchor` / `md.anchor_order_mismatch` | `instructions.md` anchors don't line up with the steps |

Issues marked *fixable* can be resolved with **Scaffold missing files** in the
in-app editor.

### Ship

| Audience | How |
|---|---|
| One user, one machine | Copy the `.import` into their app-data `profiles/` folder |
| Everyone, forever | Add the source under `profiles/src/`, run `build.sh`, add the filename to `BUILTIN_PROFILES` in `src-tauri/src/profile.rs`, rebuild |

Built-ins and user profiles can share an `id`; the sidebar keys on the bundle
path, so a duplicate name is expected and harmless.

---

## 12. Copy from an example

Every example under `profiles/src/` is a built-in, is verified by `build.sh`,
and is heavily commented in place. Start from whichever is closest.

| Example | Demonstrates |
|---|---|
| `test1` | The minimum viable import: one input, one transform, one output, one instruction step |
| `test2` | Multiple inputs (CSV **and** XLSX) and multiple `transforms:` in one step |
| `test3` | `notices` — post-run tables for follow-up items |
| `test4` | Multi-output: one transform, three `COPY … TO '{{output:…}}'` targets |
| `re_query_demo` | `re_query`: `params_sql` → `{{rows:col}}` → RE → `{{query:Label}}` joined into the import file |
| `code_table_demo` | `code_tables:` pull + a `code_table_sync` step |
| `code_table_crossref` | The full loop: audit codes against RE, offer to create the missing ones, then read `{{sync:…}}` back so the new ids land in the import file |
| `gift_activity` | The complete report profile: parameters → query → transform → table viz → write-back action |

```bash
cp -r profiles/src/test1 profiles/src/vendor_a
# edit id/name in structure.yaml, then:
./profiles/build.sh
```

---

## 13. Troubleshooting

| Symptom | Cause and fix |
|---|---|
| DuckDB "Referenced column not found" | Column name needs double quotes: `"Item #"` |
| Excel columns come back as `column0`, `column1` | Headers aren't on row 1 — skip rows with a CTE or `OFFSET` |
| `{{input_file}}` errors in a working transform | The transform declares more than one input — switch to `{{input:Label}}` |
| Binder error on a `{{sync:X}}` column | The sync attempted zero rows, so there's no schema to infer — use `read_json(..., columns={...})` |
| `{{query:X}}` or `{{sync:X}}` unresolved at runtime | The consumer didn't declare it in `query_input` / `sync_input`, or the producer step comes later, or the producer hasn't been run yet (a zero-row sync still publishes `[]`, but it must have run) |
| `{{codetable:X}}` errors | `X` must match a `code_tables:` entry's `output`, not the RE table's display name |
| `{{rows:col}}` sent as a string, not an array | It only becomes an array when the YAML value is *exactly* the placeholder |
| Code table step "does nothing" | Check the live/mock badge — with no RE connection (or `RE_NXT_MOCK=1`) writes are stubbed |
| RE returns `400 … "The Query field is required"` | You posted a bare definition. `template` should be an `ExecuteQueryDefinition`; the runtime wraps it. Don't hand-roll the envelope unless you supply a top-level `query` key |
| Query returns everything, ignoring the filter | `{{rows:col}}` named a column `params_sql` doesn't return, so it wasn't substituted — check the alias |
| Live results have `"$5.00"` / localized dates | Something overrode `formatting_mode: None` |
| Report chart renders as an empty dashed box | Only `table` is implemented; `bar`/`line`/`pie`/`kpi` are placeholders |
| A built-in profile edit doesn't show up | Run `./profiles/build.sh` and restart `npm run tauri dev` — built-ins are compiled into the binary |
| Two profiles with the same name in the sidebar | A built-in and a user profile share an `id`. Expected — selection keys on the bundle path |

---

## 14. Author's checklists

### Import profile

- [ ] `id`, `name`, `version`, `min_app_version` set; no `kind:` key
- [ ] Every `inputs[].label` is referenced by a `file_input` step
- [ ] Every input with `validate: true` has `validation:` rules
- [ ] Every `outputs[].label` is written by some transform
- [ ] Every `sql:` names a file that exists under `sql/`
- [ ] Multi-output SQL uses one `COPY … TO '{{output:Label}}'` per output
- [ ] Every `{{query:…}}` / `{{sync:…}}` is declared in `query_input` / `sync_input`, and its producer step comes earlier
- [ ] Fixtures exist for every `re_query` (`fixtures/queries/`) and code table (`fixtures/codetables/`)
- [ ] `instructions.md` has an anchor per step, in step order
- [ ] `./profiles/build.sh` passes
- [ ] Run end to end against a file in `test-files/`, in mock mode and live

### Report profile

- [ ] `kind: report` set
- [ ] Every `parameters[].id` used by a query is spelled the same in `{{param:…}}`
- [ ] `select` parameters have `options:`
- [ ] Every query has `ref` and/or `template`, and a unique `output`
- [ ] `query_field_id` / `type_id` values are from **your** environment
- [ ] Every `user_alias` matches the column name the transform SQL selects
- [ ] Every transform `input:` names a declared query output
- [ ] Every visualization's `data:` names a transform output, and every
      `config.columns[].field` matches a selected column
- [ ] Visualizations are `type: table` (charts are still placeholders)
- [ ] Actions supply `id_field`, `type_id`, and `id_query_field_id`
- [ ] `fixtures/<output>.json` exists for every query
- [ ] `./profiles/build.sh` passes
- [ ] Refresh works in mock mode and live; check the badge
