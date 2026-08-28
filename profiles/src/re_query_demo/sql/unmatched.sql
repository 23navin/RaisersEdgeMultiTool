-- Vendor rows RE didn't return — surfaced as a notice, not an error.
SELECT
    v."record_id" AS record_id,
    v."new_email" AS new_email
FROM read_csv_auto('{{input:Vendor}}') v
LEFT JOIN read_json_auto('{{query:RERecords}}') r
       ON CAST(r.re_id AS VARCHAR) = CAST(v."record_id" AS VARCHAR)
WHERE r.re_id IS NULL
ORDER BY v."record_id";
