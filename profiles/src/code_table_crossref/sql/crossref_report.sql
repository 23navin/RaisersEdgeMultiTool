-- Audit file: one row per distinct constituent_code in the upload, flagged
-- against RE's Constituent Codes table.
--
-- Unlike the notice query beside it, this keeps the matched values too — the
-- point of the downloadable audit is to show the whole cross-reference, not
-- just the failures.
--
-- {{input:Label}}     — a file the user uploaded
-- {{codetable:Label}} — a code table pulled from RE (see structure.yaml)
WITH file_codes AS (
    SELECT
        trim(coalesce(c."constituent_code", '')) AS file_value,
        count(*)                                 AS occurrences,
        min(c."constituent_id")                  AS first_record_id
    FROM read_csv_auto('{{input:Constituents}}') c
    -- Blank codes aren't missing entries, they're rows with nothing to match.
    WHERE trim(coalesce(c."constituent_code", '')) <> ''
    GROUP BY 1
)
SELECT
    f.file_value,
    f.occurrences,
    f.first_record_id,
    CASE WHEN ct.table_entries_id IS NULL THEN 'MISSING' ELSE 'OK' END AS status,
    ct.table_entries_id  AS re_entry_id,
    ct.long_description  AS re_entry_name
FROM file_codes f
-- Case- and whitespace-insensitive, so "board member " matches "Board Member".
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(f.file_value)
ORDER BY status, f.file_value;
