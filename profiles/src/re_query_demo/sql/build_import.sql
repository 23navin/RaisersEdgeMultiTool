-- Joins the vendor file against what RE returned. {{query:RERecords}} resolves
-- to the JSON the re_query step wrote; read it like any other file.
SELECT
    v."record_id"                       AS record_id,
    r.re_name                           AS current_name,
    r.re_email                          AS current_email,
    v."new_email"                       AS new_email
FROM read_csv_auto('{{input:Vendor}}') v
INNER JOIN read_json_auto('{{query:RERecords}}') r
        ON CAST(r.re_id AS VARCHAR) = CAST(v."record_id" AS VARCHAR)
WHERE lower(trim(r.re_email)) <> lower(trim(v."new_email"))
ORDER BY v."record_id";
