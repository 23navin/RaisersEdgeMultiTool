# Step Types Reference

This is the working reference for **what step types exist today**, **how each is
represented in `structure.yaml` and `instructions.md`**, and **how each is
rendered in the UI**. Use this when planning changes — both YAML shape and
component behavior live here side by side.

Authoritative code locations:
- YAML parsing (Rust): `src-tauri/src/profile.rs`
- Type mirror (TS): `src/types.ts`
- Step → component dispatch: `src/components/imports/MainPanel.tsx` (`switch (step.type)`)
- Instruction section parser: `parse_instructions()` in `src-tauri/src/profile.rs`
- Step-type acceptance: `src-tauri/src/validate.rs` and `profiles/build.sh` — both
  reject unknown types

**Scope:** this file covers **import** profiles. Report profiles (`kind: report`)
have no steps at all — they declare `parameters` / `queries` / `transforms` /
`visualizations` / `actions`, documented in `REPORT_PROFILES.md`. The one section
below that applies to both kinds is *Code tables*.

---

## Profile bundle layout

A `.import` file is a zip archive containing:

```
structure.yaml      # required — profile metadata, inputs/outputs, steps
instructions.md     # optional — markdown text per step
sql/                # optional — .sql files referenced by sql_transform steps,
                    #            code_table_sync steps, and notice queries
fixtures/           # optional — mock-mode RE responses (see Code tables below)
assets/             # optional — images referenced from instructions.md
```

---

## structure.yaml — top-level shape

```yaml
id: vendor_a
name: "Vendor A Import"
version: "1.0"
min_app_version: "0.1.0"

inputs:
  - label: Classification
    type: csv                # "csv" | "xlsx"
    required: true
    validation:              # optional column-level rules
      - label: "Item #"
        required: true
        type: number         # "string" | "number"
        digits: 6            # number only — exact digit count
      - label: Category
        required: true
        type: string
        value: ["Alpha", "Beta", "Gamma"]   # restrict to these values

outputs:
  - label: Update_Records
    type: csv

code_tables:               # optional — RE code tables pulled before any SQL runs
  - id: constituent_codes  # (see Code tables below; also valid in report profiles)
    name: "Constituent Codes"
    output: ConstituentCodes

steps:
  - label: ...
    type: ...
    # type-specific fields here
```

---

## instructions.md — anchor convention

The markdown file is one document split into sections by HTML anchor comments:

```markdown
# Profile Title

One-line description of the profile.

---

<!-- label: StepLabelFromYaml -->
## Section heading

Body markdown for this step…

<!-- label: NextStepLabel -->
## Next heading
…
```

- Text **before** the first `<!-- label: -->` is stored under the key
  `_header` and shown as the profile description in `MainPanel`.
- Everything between one anchor and the next belongs to the step whose `label`
  matches the anchor.
- The `##` heading inside a section is what the UI displays as the step name.
  If absent, the YAML `label` is shown.
- Inline backticks render as code chips in the rendered prose.
- `![alt](assets/path.png)` images resolve against the profile's extracted
  `temp_dir` and render in `manual_instruction` sections.

---

## Step type: `file_input`

### Purpose
Prompts the user to attach a file for one of the profile's `inputs`. Renders
the **Upload + filename box + Validate** row (`StepSelectFiles`).

### YAML
```yaml
- label: AddSourceFiles
  type: file_input
  input:
    - label: Classification     # must match an entry under top-level `inputs:`
      validate: true            # enables the Validate button for this row
    - label: Categories         # multiple inputs render as stacked rows
      validate: false           # in the same card
```

`input` is an array — each entry renders as its own upload row inside the
step's card. Use this when two files come from the same source (e.g. a
vendor portal export containing both inventory and category lookups).

### Markdown
```markdown
<!-- label: AddSourceFiles -->
## Select Source Files

Upload the un-edited data file (typically `file_from_vendor.csv`) that the
vendor provided.
```

The section body becomes the description shown above the upload row.

### UI behavior
- **Upload** button: black until a file is attached, then grey.
- **Filename box**: shows the attached filename; has an inline **X** to clear.
- **Validate** button:
  - light grey when nothing is uploaded (disabled)
  - black when a file is attached and not yet validated
  - green check when validated successfully
  - red X when validation failed
- **Validation errors table** — when `fileStatus === "invalid"`, a data table
  appears below the file row listing each error with columns: `Row`,
  `Column`, `Value`, `Error`. The shape is defined by `ValidationError` in
  `src/types.ts`. Backend should return one row per failing cell.
- Re-uploading or re-validating resets the status of any downstream
  `sql_transform` that consumes this input.

---

## Step type: `sql_transform`

### Purpose
Runs a SQL file against the attached inputs (via DuckDB) and writes output CSVs.
Renders the **pipeline diagram + Generate/progress/Download** row
(`StepGenerateFile`).

### YAML — single transform (shortcut form)
```yaml
- label: CreateImportFile
  type: sql_transform
  input:
    - Classification           # short form — just the input label
    # or: { label: Classification, validate: true }
  sql: primary_transform.sql   # filename inside the bundle's sql/ folder
  output:
    - Update_Records           # must match top-level `outputs:`
```

### YAML — multiple transforms in one step
```yaml
- label: BuildOutputs
  type: sql_transform
  transforms:
    - input: [Inventory, Categories]
      sql: inventory.sql
      output: [Inventory_Import]
    - input: [Pricing]
      sql: pricing.sql
      output: [Pricing_Update]
```

### YAML — transform with notices
```yaml
- label: BuildCatalog
  type: sql_transform
  input: [Catalog]
  sql: catalog.sql
  output: [Catalog_Import]
  notices:
    - label: "Unrecognized Categories"
      sql: notices/unknown_categories.sql
      description: >-
        These category values aren't in the master list. Add them in the
        Catalog Admin tool before importing.
```

- `notices` is an array of informational queries that run **after** the main
  transform succeeds. They surface data that's nominally valid but needs
  external follow-up (new lookup values, unit-of-measure changes, etc.) —
  they do NOT mark the transform as failed.
- Each notice has `label` (heading), `sql` (filename inside the bundle's
  `sql/` folder), and optional `description` (sub-heading prose).
- The notice SQL is expected to return zero rows in the nominal case. Any
  returned rows are rendered as a table beneath the Generate row using the
  result-set column names as headers.
- `{{input_file}}` / `{{input:Label}}` substitution works the same way as in the
  main transform, and so does `{{codetable:Label}}`.
- Notices live on the individual transform — both the single-transform
  shortcut and entries inside `transforms[]` support a `notices` field.

- Each transform produces its own pipeline diagram + Generate/Download
  row inside the step's card, with a thin divider between them.
- Each transform is generated independently — `canGenerate`,
  `generateStatus`, and `generateProgress` are tracked per transform.
- When `transforms` is present, the step-level `input`/`sql`/`output`
  fields are ignored.
- `input` accepts both short-form (`"Classification"`) and long-form
  (`{ label: "Classification", validate: true }`) entries.
- `sql` is required — names a `.sql` file inside the bundle.
- `output` is an array — one transform can produce multiple outputs.

### SQL placeholders
Inside the SQL file:
- `{{input_file}}` is replaced at runtime with the attached file's path.
- DuckDB reads files directly: `read_csv_auto('{{input_file}}')`,
  `read_xlsx('{{input_file}}')`.
- Quote column names containing spaces or special characters:
  `"Item #"`.
- `{{output:LabelName}}` is replaced with the temp-dir path for the declared
  output named `LabelName`. Use this when a single transform writes multiple
  files — the SQL author writes one `COPY` per output and the runtime
  executes them as a batch.

### Single vs multi output

A transform's `output:` array is the list of files it produces. Two modes
based on whether the SQL contains `{{output:Label}}` placeholders:

**Single-output (legacy shortcut).** SQL is one bare `SELECT`. The runtime
wraps it as `COPY (<your select>) TO '<path>'` and writes the lone output.
Requires exactly one entry in `output:`.

```sql
SELECT ... FROM read_csv_auto('{{input_file}}');
```

**Multi-output.** SQL contains one `COPY` statement per output, each
targeting a `{{output:LabelName}}` placeholder that matches an entry in
`output:`. Runs via DuckDB `execute_batch`, so any DuckDB-valid sequence of
statements (CTEs, `CREATE TEMP TABLE`, intermediate `SELECT`s, then multiple
`COPY`s) works.

```sql
COPY (SELECT ... FROM read_csv_auto('{{input_file}}'))
  TO '{{output:Inventory_Import}}' (HEADER, DELIMITER ',');

COPY (SELECT ... FROM read_csv_auto('{{input_file}}'))
  TO '{{output:Pricing_Update}}' (HEADER, DELIMITER ',');
```

The UI renders one Download button per output beneath the Generate row, each
labeled with the output's name.

### Markdown
```markdown
<!-- label: CreateImportFile -->
## Generate Import File

Optional body describing what this transform does.
```

### UI behavior
- **Pipeline diagram**: lists each input pill on the left, database icon in
  the middle, each output pill on the right. A small green check (ready) or
  red X (not ready) sits next to each pill.
  - Input pill is green only when its file is uploaded **and** valid.
  - Output pill is green only after this transform's generation has
    completed successfully.
- **Generate** button: black when all required inputs are valid; light grey
  otherwise; grey once generation is done; black again after an error
  (re-runnable).
- **Progress bar**: fills as generation runs; turns **green** on success,
  **red** on error.
- **Download** button: light grey until generation finishes, then black. Stays
  light grey after an error.
- **SQL errors table** — when `generateStatus === "error"`, a data table
  appears below the Generate/Download row listing each `SqlError` from
  `src/types.ts` with columns: `Line`, `Type`, `Message`. The backend
  should return one row per DuckDB / SQL error.
- **Notices** — when `generateStatus === "done"`, any non-empty `Notice`
  from the backend renders as an amber callout beneath the Generate/Download
  row. Each callout shows the notice's `label` and `description`, then a
  table whose columns/rows come straight from the notice query's result set.
  Notices are informational only — they don't change `done` status or
  prevent the user from moving on, but they should be addressed externally.

---

## Step type: `code_table_sync`

### Purpose
Pushes rows to an RE **code table** — creating, updating, or deleting entries.
The step's SQL selects the rows; each returned row becomes one write. There is
no output file. Renders via `StepCodeTableSync`.

Reading a code table needs no step at all — declare it in the top-level
`code_tables:` section and reference it from any SQL as `{{codetable:Label}}`
(see *Code tables* below).

### YAML
```yaml
  - label: AddMissingCodes
    type: code_table_sync
    code_table: "Constituent Codes"   # exact name — or code_table_id: "43"
    operation: create                 # create | update | delete
    input:                            # inputs the SQL reads
      - Classification
    sql: missing_codes.sql            # selects the rows to push
```

### The SQL contract
Column names are the API's writable `TableEntry` fields. Any other column is
ignored, so a query can carry extra columns for its own joins.

| Column              | Applies to           | Notes                                    |
| ------------------- | -------------------- | ---------------------------------------- |
| `long_description`  | create (**required**), update | The entry's name.               |
| `table_entries_id`  | update, delete (**required**) | RE's system id for the entry.   |
| `short_description` | create, update       | Optional.                                |
| `numeric_value`     | create, update       | Optional; parsed as a number.            |
| `sequence`          | create               | Optional; parsed as an integer.          |
| `is_active`         | create, update       | Optional; `true`/`1`/`yes` → `true`.     |

### Behavior
- One HTTP call per row. A failed row is recorded and the run **continues** —
  the result carries `succeeded`, `attempted`, and a `failures[]` list naming
  each bad row, so one rejected entry can't hide the rest.
- Writes reach RE only when connected and `RE_NXT_MOCK` is unset; otherwise
  they're stubbed. The result's `mode` (`live`/`mock`) is shown as a badge
  next to the button *before* the user clicks, because this step changes RE data.
- The step is marked done only when every row succeeded.

### UI behavior
- **Pipeline diagram**: input pills on the left, an upload icon in the middle,
  the target code table on the right.
- **Button**: labelled by operation ("Add entries" / "Update entries" /
  "Delete entries"), enabled on the same rule as a transform — every required
  input uploaded and valid.
- **Result**: a green callout on full success, amber on partial. Failures render
  as a table of `Row` / `Entry` / `Error`.

---

## Code tables (no step required)

A top-level section, valid in **both** import and report profiles. Each entry is
fetched from RE before any SQL runs and written to the run's temp dir as JSON.

```yaml
code_tables:
  - id: constituent_codes
    name: "Constituent Codes"    # exact name — or code_table_id: "43"
    include_inactive: false      # default false
    output: ConstituentCodes     # → {{codetable:ConstituentCodes}}
```

`{{codetable:Label}}` resolves to that JSON file, read with `read_json_auto`:

```sql
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(trim(v."class"))
```

Columns are the API's `TableEntry` fields: `table_entries_id`,
`long_description`, `short_description`, `numeric_value`, `sequence`,
`is_active`, `is_system_entry`, `code_tables_id`, `code_tables_name`.

Mock mode reads `fixtures/codetables/<output>.json` from the bundle, so a
profile that uses code tables still runs offline. Working example:
`profiles/src/code_table_demo/`.

---

## Step type: `manual_instruction`

### Purpose
Renders markdown-only content — no file action. Used for closing instructions
(e.g. "now open the BulkImport tool and run X"). Renders via `StepImport`.

### YAML
```yaml
- label: Import
  type: manual_instruction
```

No type-specific fields. Just `label` and `type`.

### Markdown
```markdown
<!-- label: Import -->
## Import into database

Import into the database using the `SimpleImport` profile.

![Import tool](assets/import_profile.png)

If there are exceptions, contact `vendor@example.com`.
```

### UI behavior
- The `##` heading is shown as the step heading.
- Body renders as prose: paragraphs, inline `code`, images.
- Images load from `temp_dir/assets/...` via Tauri's `convertFileSrc`.

---

## Step completion (sidebar checkmark)

Computed in `App.tsx` (`stepsDone`):
- `file_input` — done when **every** input row in the step is `"valid"`.
- `sql_transform` — done when **every** transform in the step has
  `status === "done"`.
- `code_table_sync` — done when the run finished **and** every row succeeded
  (`status === "done" && result.ok`). A partial run leaves the step open.
- `manual_instruction` — currently never marked done (no user action tracked).

Generation state is keyed by `${stepLabel}::${transformIdx}` so that
multi-transform steps track each transform independently. Sync state is keyed by
step label alone — a `code_table_sync` step holds exactly one operation.

---

## Quick reference — required vs optional fields per step

| Step type            | Required fields                      | Optional fields                          |
| -------------------- | ------------------------------------ | ---------------------------------------- |
| `file_input`         | `label`, `type`, at least one `input`| `input[].validate`                       |
| `sql_transform`      | `label`, `type`, `sql` or `transforms`| `input`, `output`, `notices`, `transforms[].input`, `transforms[].output`, `transforms[].notices` |
| `code_table_sync`    | `label`, `type`, `sql`, `operation`, `code_table` or `code_table_id` | `input`     |
| `manual_instruction` | `label`, `type`                      | —                                        |

---

## When extending step types

To add a new step type or change an existing one, touch:

1. **`src-tauri/src/profile.rs`** — extend `Step` / `StepInputRef` if new fields are needed; serde handles the YAML mapping.
2. **`src/types.ts`** — mirror any new field in the TS `Step` type.
3. **`src/components/imports/MainPanel.tsx`** — add a `case "your_type":` in the
   `StepSection` switch, dispatching to a new or existing component.
4. **`src/App.tsx`** — extend state shape / handlers if the new step needs
   to track per-step data beyond the existing `files` and `generations` maps.
5. **`src-tauri/src/validate.rs`** and **`profiles/build.sh`** — both reject
   unknown step types, so a new one must be added to each or bundles won't verify.
6. **An example profile** — add a corresponding YAML+MD example under
   `profiles/src/<name>/` and rebuild with `profiles/build.sh` so you can
   exercise it end to end.
