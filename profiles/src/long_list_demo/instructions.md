# Long List Demo

Exercises the result tables in this workspace when they run long. Every table
caps at ten visible rows and scrolls from there, and only those ten animate in —
the rest are already drawn when you scroll to them.

Test files are in this profile's `test-files/` folder.

<!-- label: ValidateFiles -->
## Upload the three test files

Upload and validate each one — each is a different length of list.

- `roster_messy.csv` → **Messy Roster**. Returns **26 errors** and a **14-row**
  column-name notice, so both tables scroll. The header row stays put while you
  scroll, and rows 11 and on appear without the stagger.
- `extract_short.csv` → **Short Extract**. Returns **3 errors** — under the cut,
  so the card grows to fit and every row animates in, unchanged.
- `roster_clean.csv` → **Clean Roster**. Validates clean, which is what lets the
  next step run.

The messy roster earns its errors honestly: every column it carries is named a
little differently from the rule that checks it (`constituent_id`, `LASTNAME`,
`Gift Amount ($)`, `campaign!`), which is what fills the notice, and two rules —
`Appeal Code` and `Reference` — have no column at all.

<!-- label: GenerateImportFile -->
## Generate the import file

Six of the thirty Clean Roster rows belong in the import file. The other 24 are
listed in the notice below it, one row each, so that table scrolls too — note
that the notice's label banner stays put and only the rows move.

<!-- label: Import -->
## Import into the database

Nothing to import — this profile is a UI fixture. The generated file is real,
though: six rows, one per gift that passed.
