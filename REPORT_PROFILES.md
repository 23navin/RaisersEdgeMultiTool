# Report Profiles

Report profiles produce **real-time operational reports**. A report pulls data
from the RE NXT (Blackbaud SKY) API, processes it in DuckDB with SQL, and renders
interactive visualizations. Inputs are deliberately kept separate from
visualizations: a **Refresh** button runs the data pipeline and feeds the
visualizations, which start blank.

Report profiles reuse the existing `.import` bundle format and loader
(`structure.yaml` + `sql/` + `instructions.md`, see `profile.rs`). They are
distinguished by a top-level `kind: report` and replace the import step-list with
five declarative sections.

> **Status:** This document defines the *structure/contract*. The backend parses
> report bundles (deserialize-only) but **does not execute them yet** — the RE
> call runner, JSON→DuckDB path, refresh pipeline, and write-back actions are a
> follow-up. See the plan at `~/.claude/plans/recommend-a-structure-for-adaptive-frost.md`.

---

## The `kind` discriminator

```yaml
kind: report      # absent or "import" => the import workflow (inputs/outputs/steps)
```

Import bundles omit `kind` (or set `import`) and keep using `inputs`, `outputs`,
`steps`. Report bundles set `kind: report` and use the sections below; their
`inputs`/`outputs`/`steps` are empty.

---

## The five sections

| Section | Purpose | UI region |
|---|---|---|
| `parameters` | UI inputs (date range, select, text…) | Inputs panel |
| `queries` | parameterized RE API calls (hybrid library) | — (run on Refresh) |
| `transforms` | DuckDB SQL over query results | — (run on Refresh) |
| `visualizations` | shared viz components bound to transform outputs | Visualizations area |
| `actions` | on-demand write-backs (e.g. create an RE query) | buttons near viz |

### Pipeline

```
parameters ──▶ queries (RE API) ──▶ transforms (DuckDB SQL) ──▶ visualizations
   inputs        JSON results          result sets (cols+rows)      rendered
                                                                        │
actions (on demand) ◀───────────────────────────────────────────────── result sets
```

**Refresh model:** visualizations are blank until the user presses **Refresh**,
which runs `queries → transforms` and feeds the results into the visualizations.
Changing any parameter marks the visualizations *stale* (overlay prompting a
re-refresh). `actions` are separate on-demand buttons — they are **not** part of
Refresh.

---

## `parameters` — UI inputs

```yaml
parameters:
  - id: gift_dates
    label: "Gift Date Range"
    type: date_range          # date | date_range | select | text | number
    required: true
    default:                  # optional; shape varies by type
      preset: last_30_days
  - id: fund
    label: "Fund"
    type: select
    options: ["Annual", "Capital", "Endowment"]
```

A `date_range` exposes `.from` and `.to` for placeholders. `select` uses
`options`. Parameter `id`s are referenced from queries via `{{param:id}}`.

---

## `queries` — RE API calls (hybrid library)

Each query references the shared RE call library and binds parameters into it.

```yaml
queries:
  - id: gift_rows
    ref: re.query.execute            # central registry call id
    bind:
      query_id: 12345                # literal, or an inline ad-hoc definition
      "filters.gift_date.from": "{{param:gift_dates.from}}"
      "filters.gift_date.to": "{{param:gift_dates.to}}"
    output: GiftRows                 # JSON result, referenced by transforms
```

- **`ref`** — id of a call in the **central registry** (the common, parameterized
  library shared with the Data Requests tab).
- **`template`** *(alternative to `ref`)* — an inline, bundle-local call
  definition (the per-bundle half of the **hybrid** model). A bundle may also
  ship call definitions under `calls/` that override/extend the central registry.
- **`bind`** — maps parameter values and literals into the call. Dotted keys
  address nested request fields. Values support `{{param:...}}` and `{{now}}`.
- **`output`** — names the JSON result that transforms read via `{{query:Label}}`.

The `re.query.execute` call hides the SKY **async job flow** (execute → poll job
→ page results) so authors never deal with polling.

---

## `transforms` — DuckDB SQL

Top-level `transforms` (distinct from the import `Step`'s nested `transforms`).
Reuses the existing transform engine (`db.rs`); inputs are query outputs rather
than uploaded files.

```yaml
transforms:
  - id: constituent_gifts
    input: [GiftRows]                # query output labels
    sql: constituent_gifts.sql       # file in the bundle's sql/ folder
    output: ConstituentGifts         # in-memory result set (columns + rows)
```

The SQL reads each query result with `read_json_auto` via the `{{query:Label}}`
placeholder:

```sql
SELECT constituent_name, gift_id, gift_amount, gift_date
FROM read_json_auto('{{query:GiftRows}}')
ORDER BY gift_date DESC;
```

A transform `output` is an in-memory **ResultSet** (`{ columns, rows }`, the same
shape as a `Notice`) — viz-ready, no CSV round-trip.

---

## `visualizations` — bound to transform outputs

```yaml
visualizations:
  - id: gifts_table
    type: table                      # table | bar | line | pie | kpi
    title: "Constituents & Gifts"
    data: ConstituentGifts           # transform output to render
    config:
      sortable: true
      columns:
        - { field: constituent_name, header: "Constituent" }
        - { field: gift_amount, header: "Amount", format: currency }
        - { field: gift_date, header: "Date", format: date }
```

The renderer maps `type` → component via `VIZ_REGISTRY`
(`src/components/reports/viz/index.tsx`), mirroring the imports `MainPanel`
step-dispatch. `config` is viz-specific (column defs for tables; axes/series for
charts).

---

## `actions` — on-demand write-backs

```yaml
actions:
  - id: save_re_query
    label: "Create Query in RE"
    ref: re.query.create             # API library call
    input: ConstituentGifts          # result set feeding the call
    bind:
      name: "Report Gifts {{now}}"
      id_field: gift_id              # result column supplying the ids
```

Actions run only when the user clicks them. `input` names a result set; `bind`
maps its data and literals into the call (e.g. collect `gift_id` values into the
new query's id list).

---

## Placeholders

| Placeholder | Resolves to | Used in |
|---|---|---|
| `{{param:Id}}` / `{{param:Id.from}}` / `{{param:Id.to}}` | a parameter's value | `queries[].bind` |
| `{{query:Label}}` | temp-dir JSON path of a query result (for `read_json_auto`) | transform SQL |
| `{{now}}` | current timestamp | `actions[].bind` (e.g. query naming) |

The existing import placeholders (`{{input:Label}}`, `{{input_file}}`,
`{{output:Label}}`, see `db.rs` / CLAUDE.md) are unchanged and unused by reports.

---

## Shared libraries

**A — RE API calls (hybrid).** A central registry of parameterized RE calls,
referenced by `ref` from both Report profiles and the Data Requests tab. Each
entry declares `id`, `name`, `kind` (`query_execute` | `rest_get` | `rest_post`),
typed `params`, and a `result` shape.

- Central definitions: Rust registry `src-tauri/src/re_calls.rs` — the single
  executor, with a `Transport` of **Live** (real SKY API via
  `sky_auth::live_credentials`) or **Mock** (fixture files) — mirrored by the TS
  catalog `src/lib/re-calls.ts` (drives the UI).
- Per-bundle override: a profile may ship `calls/*.yaml`; the loader merges
  bundle calls over the central registry. *(Not yet implemented.)*

**B — Visualizations.** `src/components/reports/viz/` exports
`VIZ_REGISTRY: Record<string, VizComponent>` keyed by `type`. Each component
takes `{ data: ResultSet, config, title }`. Intended stack: TanStack Table for
`table`, Recharts for `bar`/`line`/`pie`/`kpi`.

---

## Bundle layout

```
profiles/src/gift_activity/
├── structure.yaml          # kind: report + the five sections
├── instructions.md         # header + optional <!-- label: <viz id> --> sections
├── sql/
│   └── constituent_gifts.sql
└── fixtures/               # RE responses for mock mode, one per query output
    └── GiftRows.json
```

`instructions.md` anchors are keyed by section id (e.g. a visualization `id`),
following the same `<!-- label: X -->` convention the import parser uses.

See `profiles/src/gift_activity/` for a complete worked example (the Gift
Activity Report).

---

## Execution

The report pipeline runs end-to-end. The RE call goes through a `Transport`
chosen by the command layer:

- **Live** — used when a RE NXT connection exists (`sky_auth::has_connection`),
  unless the `RE_NXT_MOCK` env var is set. Calls the real SKY Query API.
- **Mock** — fixture files; the offline/dev/test fallback.

`ReportRunResult.mode` (`"live"` | `"mock"`) reports which one ran; the Reports
tab shows it as a badge.

**Pipeline** (`src-tauri/src/report.rs`):

1. **Params** — supplied `param_values` (falling back to each parameter's
   `default`) become substitutions: `{{param:id}}` for scalars, `{{param:id.key}}`
   for object values (e.g. a `date_range` yields `.from` / `.to`).
2. **Queries** — the ad-hoc `template` (or `bind`) is substituted with those
   params to form the request, then `re_calls::execute_query` returns the rows.
   - *Live:* `POST /query/queries/execute?product=RE&module=None` →
     poll `GET /query/jobs/{id}` until `status` is completed →
     download `sas_uri` (no auth headers) → rows. Headers: `Authorization:
     Bearer <token>` + `Bb-Api-Subscription-Key`.
   - *Mock:* reads `fixtures/<query.output>.json`.
   Either way the result is normalized to a JSON array of row objects and written
   to a per-run temp dir.
3. **Transforms** — `{{query:Label}}` resolves to the query result's JSON path;
   the SQL runs via DuckDB `read_json_auto` and yields a `ResultSet`
   (`db::query_to_result_set`).
4. **Result** — `ResultSet`s keyed by transform `output` (what a visualization's
   `data` binds to), plus per-query debug info (the resolved request + row count)
   and `mode`.

**Actions** (write-back) re-run the pipeline, collect the `id_field` column from
the action's `input` result set, and `re_calls::create_query` POSTs `/query/queries`
(live) or returns a stub (mock).

**Adjusting to your environment:** SKY paths / params (`API_BASE`, `EXECUTE_PATH`,
`JOB_PATH`, `CREATE_PATH`, `EXECUTE_QUERY_PARAMS`) are constants at the top of
`re_calls.rs`. Live result shapes are coerced defensively by `normalize_rows`
(array, `{rows|results|value|data|records}`, or `{fields, rows}`). The
create-query body is the least-documented part — verify its shape against your env.

**Fixtures convention:** `<bundle>/fixtures/<query.output>.json` — a JSON array of
row objects, resolved against the extracted bundle (`loaded.temp_dir`).

**Commands** (`commands.rs`, registered in `main.rs`; both `async`):

| Command | Args | Returns |
|---|---|---|
| `run_report` | `zipPath`, `paramValues` (map) | `ReportRunResult` (`data`, `queries`, `generated_at`, `mode`) |
| `run_report_action` | `zipPath`, `actionId`, `paramValues` | `ActionResult` (`ok`, `message`) |

`zipPath` is the extracted temp dir (`loadedProfile.temp_dir`), same as
`run_profile`. Force mock during live testing with `RE_NXT_MOCK=1`.

---

## Types

- Rust (deserialize-only): `Parameter`, `QueryRef`, `ReportTransform`,
  `Visualization`, `Action`, plus the `kind` + report fields on
  `ProfileStructure` in `src-tauri/src/profile.rs`.
- TypeScript (mirror): the same in `src/types.ts`, plus `ResultSet`,
  `ReportRunResult`, `QueryDebug`, `ActionResult`.
- Execution (Rust): `db::ResultSet` + `db::query_to_result_set`,
  `report::{run_report, run_report_action}`, `re_calls::{Transport, execute_query,
  create_query}`, `sky_auth::{has_connection, live_credentials}`.
