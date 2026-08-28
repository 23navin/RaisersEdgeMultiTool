-- Supplies the values the RE query filters on. Each column here becomes
-- available to the step's `template` as {{rows:<column>}} (all values, as a
-- JSON array) and {{value:<column>}} (the first row's cell).
SELECT DISTINCT
    CAST(v."record_id" AS VARCHAR) AS record_id
FROM read_csv_auto('{{input:Vendor}}') v
WHERE v."record_id" IS NOT NULL
ORDER BY record_id;
