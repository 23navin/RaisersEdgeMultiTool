# Code Table Cross-Reference Demo

Checks an uploaded file's code column against a live RE code table *before*
building the import file: report what's missing, offer to create it, then
generate.

---

<!-- label: AddSourceFile -->
## Select Source File

Upload the constituent file. It needs `constituent_id`, `last_name`,
`first_name`, and `constituent_code` columns — `sample_constituents.csv` in the
bundle's `test-files/` folder is a working example.

Validation checks that every row has a 5-digit `constituent_id` and a
`last_name`. The code column is allowed to be blank; blank codes are skipped by
every step below rather than reported as missing.

<!-- label: CheckCodeTable -->
## Cross-Reference Codes

Pulls RE's **Constituent Codes** table and compares every distinct
`constituent_code` in your file against it.

Two things come out of this step:

- **`Code_Check`** — a downloadable audit of *all* the codes found, each marked
  `OK` or `MISSING`, with the RE entry id where one exists.
- **Codes not available in RE** — the notice below the Generate row, listing
  only the unavailable values and how many rows use each. When that notice
  doesn't appear, every code in the file already exists and you can skip
  straight to generating the import file.

Matching ignores case and surrounding whitespace, so `board member ` matches
RE's `Board Member`.

<!-- label: AddMissingCodes -->
## Add Missing Codes

Creates one Constituent Codes entry in RE for each value the previous step
flagged as missing, using the value as the entry name and its first three
characters as the short description.

This step **writes to RE** — check the badge next to the button first. In mock
mode nothing is sent, and each attempt is stubbed out. Rows are pushed one at a
time; if one is rejected the rest still run, and the failures come back in a
table naming each bad row.

The step records what happened to every row it attempted — including the id RE
assigned to each new entry — and publishes it as `NewCodes`, which the next step
reads. Run this step even when nothing is missing: it attempts zero rows, records
an empty result, and the next step needs that result to exist.

<!-- label: CreateImportFile -->
## Generate Import File

Builds the import file, joining each row to its code table entry so the output
carries `re_code_id` rather than the free-text code name.

Each row's id comes from one of two places, whichever has it:

- the **Constituent Codes** table, pulled fresh on every Generate — the entries
  RE already had
- the previous step's `NewCodes` result — the ids RE assigned to the entries it
  just created, taken straight from the write responses

The second is why this resolves right away instead of depending on a re-pull,
and why it behaves the same offline: mock mode's code table fixture never
changes, but the sync still reports the ids it would have created.

Anything still unmatched — a row whose create failed, or a value the sync was
never given — is listed in the **Rows still without a code** notice, with the
error RE returned.

<!-- label: Import -->
## Import into database

Run the generated `Import_File` through the BulkImport tool.

Keep the `Code_Check` audit alongside it — it's the record of which codes were
already in RE and which ones this run created.
