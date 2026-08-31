-- Source rows for the AddMissingCodes step: each row returned here becomes one
-- created entry in RE's Constituent Codes table.
--
-- Same set of values as missing_values.sql, but the column names are the API's
-- writable TableEntry fields rather than UI headings. For `create` only
-- long_description is required; short_description, numeric_value, sequence and
-- is_active are optional, and any other column is ignored.
SELECT DISTINCT
    trim(c."constituent_code")                       AS long_description,
    upper(substr(trim(c."constituent_code"), 1, 3))  AS short_description,
    true                                             AS is_active
FROM read_csv_auto('{{input:Constituents}}') c
WHERE trim(coalesce(c."constituent_code", '')) <> ''
  AND lower(trim(c."constituent_code")) NOT IN (
      SELECT lower(trim(long_description))
      FROM read_json_auto('{{codetable:ConstituentCodes}}')
      WHERE long_description IS NOT NULL
  )
ORDER BY long_description;
