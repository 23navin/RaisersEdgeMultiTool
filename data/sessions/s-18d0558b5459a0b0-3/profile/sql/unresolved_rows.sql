-- Notice on the CreateImportFile step: rows that named a code RE still doesn't
-- have, after both the pulled table and the sync step's own results.
--
-- A row shows up here when the sync failed for that value, or when the value
-- was never offered to the sync at all. Blank codes are excluded — a row with
-- no code isn't an unresolved one.
SELECT
    c."constituent_id"         AS "Record Id",
    trim(c."last_name")        AS "Last Name",
    trim(c."constituent_code") AS "Unmatched Code",
    coalesce(n.sync_error, '') AS "Sync Error"
FROM read_csv_auto('{{input:Constituents}}') c
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(trim(c."constituent_code"))
LEFT JOIN read_json('{{sync:NewCodes}}',
                    columns={'long_description': 'VARCHAR',
                             'table_entries_id': 'VARCHAR',
                             'sync_status': 'VARCHAR',
                             'sync_error': 'VARCHAR'}) n
       ON lower(trim(n.long_description)) = lower(trim(c."constituent_code"))
WHERE trim(coalesce(c."constituent_code", '')) <> ''
  AND ct.table_entries_id IS NULL
  AND coalesce(n.sync_status, '') <> 'ok'
ORDER BY "Record Id";
