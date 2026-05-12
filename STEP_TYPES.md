# Step Types Reference

This is the working reference for **what step types exist today**, **how each is
represented in `structure.yaml` and `instructions.md`**, and **how each is
rendered in the UI**. Use this when planning changes — both YAML shape and
component behavior live here side by side.

Authoritative code locations:
- YAML parsing (Rust): `src-tauri/src/profile.rs`
- Type mirror (TS): `src/types.ts`
- Step → component dispatch: `src/components/MainPanel.tsx` (`switch (step.type)`)
- Instruction section parser: `parse_instructions()` in `src-tauri/src/profile.rs`

---

## Profile bundle layout

A `.import` file is a zip archive containing:

```
structure.yaml      # required — profile metadata, inputs/outputs, steps
instructions.md     # optional — markdown text per step
sql/                # optional — .sql files referenced by sql_transform steps
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
- `{{input_file}}` substitution works the same way as in the main transform.
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
- `manual_instruction` — currently never marked done (no user action tracked).

Generation state is keyed by `${stepLabel}::${transformIdx}` so that
multi-transform steps track each transform independently.

---

## Quick reference — required vs optional fields per step

| Step type            | Required fields                      | Optional fields                          |
| -------------------- | ------------------------------------ | ---------------------------------------- |
| `file_input`         | `label`, `type`, at least one `input`| `input[].validate`                       |
| `sql_transform`      | `label`, `type`, `sql` or `transforms`| `input`, `output`, `notices`, `transforms[].input`, `transforms[].output`, `transforms[].notices` |
| `manual_instruction` | `label`, `type`                      | —                                        |

---

## When extending step types

To add a new step type or change an existing one, touch:

1. **`src-tauri/src/profile.rs`** — extend `Step` / `StepInputRef` if new fields are needed; serde handles the YAML mapping.
2. **`src/types.ts`** — mirror any new field in the TS `Step` type.
3. **`src/components/MainPanel.tsx`** — add a `case "your_type":` in the
   `StepSection` switch, dispatching to a new or existing component.
4. **`src/App.tsx`** — extend state shape / handlers if the new step needs
   to track per-step data beyond the existing `files` and `generations` maps.
5. **An example profile** — add a corresponding YAML+MD example under
   `profiles/src/<name>/` and rebuild with `profiles/build.sh` so you can
   exercise it end to end.
