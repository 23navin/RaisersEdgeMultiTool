-- The import file. Joins each uploaded row against the code table so the output
-- carries RE's own entry id instead of the vendor's free-text code name.
--
-- Two sources of an id, coalesced:
--   {{codetable:...}} — entries RE already had. Re-pulled on every Generate, so
--                       in live mode this alone would eventually cover the new
--                       entries too.
--   {{sync:...}}      — the ids the AddMissingCodes step just created, straight
--                       from the write responses. Declared via `sync_input`.
--
-- The sync join is what makes this resolve immediately, and identically in mock
-- mode, instead of depending on a re-pull.
--
-- Note the explicit `columns=` form rather than read_json_auto: a sync result is
-- legitimately empty when nothing was missing, and auto-detection has nothing to
-- infer a schema from in that case. Naming the columns binds either way.
SELECT
    c."constituent_id"                       AS record_id,
    trim(c."last_name")                      AS last_name,
    trim(c."first_name")                     AS first_name,
    trim(coalesce(c."constituent_code", '')) AS source_code,
    coalesce(ct.table_entries_id, n.table_entries_id) AS re_code_id,
    coalesce(ct.long_description, n.long_description) AS re_code_name
FROM read_csv_auto('{{input:Constituents}}') c
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(trim(c."constituent_code"))
LEFT JOIN read_json('{{sync:NewCodes}}',
                    columns={'long_description': 'VARCHAR',
                             'table_entries_id': 'VARCHAR',
                             'sync_status': 'VARCHAR'}) n
       ON lower(trim(n.long_description)) = lower(trim(c."constituent_code"))
      AND n.sync_status = 'ok'
ORDER BY c."constituent_id";
