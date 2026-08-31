# Code Table Demo

Shows both halves of RE code table support: reading a table into SQL, and
pushing new entries back.

---

<!-- label: AddSourceFiles -->
## Select Source Files

Upload the vendor classification file.

<!-- label: CreateImportFile -->
## Generate Import File

Joins each vendor row against RE's **Constituent Codes** table so the output
carries RE's own entry id. Rows whose class has no matching entry come back
with an empty `re_code_id` and are listed in the notice below.

<!-- label: AddMissingCodes -->
## Add Missing Codes

Creates one Constituent Codes entry for each class value RE doesn't have yet.
This writes to RE — nothing happens in mock mode, and the badge next to the
button tells you which one you're in.

<!-- label: Import -->
## Import into database

Run the generated file through the BulkImport tool.
