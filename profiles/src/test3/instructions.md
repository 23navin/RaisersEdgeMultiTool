# Catalog Import

Merges the vendor's catalog export with the database. The transform surfaces
any values that need attention externally before the import is run.

---

<!-- label: UploadCatalog -->
## Upload Catalog

Drop the vendor's `catalog.csv` export. Required columns: `SKU`, `Category`,
`Unit`.

<!-- label: BuildCatalog -->
## Build Catalog Import

Generates `Catalog_Import.csv`. Watch for the **Unrecognized Categories** and
**Unit-of-Measure Changes** notices — these don't block the export but should
be resolved externally before importing.

<!-- label: Import -->
## Import into database

Run the `CatalogImport` profile in the bulk import tool with
`Catalog_Import.csv`.
