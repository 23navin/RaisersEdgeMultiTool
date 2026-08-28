# RE Query Demo

Pulls records out of Raiser's Edge based on ids in the vendor file, then builds
an update file from the differences.

---

<!-- label: AddSourceFiles -->
## Select Source Files

Upload the vendor file. It needs a `record_id` and a `new_email` column.

<!-- label: FetchFromRE -->
## Fetch Records from RE

Sends the file's `record_id` values to RE and pulls back each record's current
name and email. Nothing is written — this is a read.

<!-- label: CreateImportFile -->
## Generate Import File

Joins the vendor file against what RE returned and keeps only the rows whose
email actually changed. Vendor rows RE didn't recognise are listed as a notice.

<!-- label: Import -->
## Import into database

Run the generated file through the BulkImport tool.
