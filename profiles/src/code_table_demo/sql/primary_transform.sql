-- Joins the vendor file against the live RE code table so the output carries
-- RE's own entry id rather than the vendor's free-text class name.
--
-- {{input:Label}}     — a file the user uploaded
-- {{codetable:Label}} — a code table pulled from RE (see structure.yaml)
SELECT
    v."ID"                       AS record_id,
    trim(v."name")               AS name,
    v."class"                    AS vendor_class,
    ct.table_entries_id          AS re_code_id
FROM read_csv_auto('{{input:Classification}}') v
LEFT JOIN read_json_auto('{{codetable:ConstituentCodes}}') ct
       ON lower(trim(ct.long_description)) = lower(trim(v."class"))
ORDER BY v."ID";
