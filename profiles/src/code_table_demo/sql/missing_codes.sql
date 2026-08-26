-- Vendor class values that RE's Constituent Codes table doesn't have yet.
--
-- Doubles as the code_table_sync source: each row becomes one created entry,
-- so the column names are the API's writable TableEntry fields. Only
-- long_description is required; short_description, numeric_value, sequence,
-- and is_active are optional, and any other column is ignored.
SELECT DISTINCT
    trim(v."class")              AS long_description,
    upper(substr(trim(v."class"), 1, 3)) AS short_description,
    true                         AS is_active
FROM read_csv_auto('{{input:Classification}}') v
WHERE trim(v."class") <> ''
  AND lower(trim(v."class")) NOT IN (
      SELECT lower(trim(long_description))
      FROM read_json_auto('{{codetable:ConstituentCodes}}')
  )
ORDER BY long_description;
