-- What RE sent back, next to what the vendor file says it should be.
-- {{query:RERecords}} is the same JSON the next step joins against — a
-- visualization step reads it exactly like a transform does, it just draws the
-- rows instead of writing them to a file.
SELECT
    CAST(r.re_id AS VARCHAR)                        AS "RE ID",
    r.re_name                                       AS "Name",
    r.re_email                                      AS "Email in RE",
    v."new_email"                                   AS "Email in vendor file",
    CASE
        WHEN v."record_id" IS NULL                                THEN 'not in file'
        WHEN lower(trim(r.re_email)) = lower(trim(v."new_email")) THEN 'unchanged'
        ELSE 'will update'
    END                                             AS "Outcome"
FROM read_json_auto('{{query:RERecords}}') r
LEFT JOIN read_csv_auto('{{input:Vendor}}') v
       ON CAST(v."record_id" AS VARCHAR) = CAST(r.re_id AS VARCHAR)
ORDER BY r.re_id;
