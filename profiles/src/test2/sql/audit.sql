-- Audit_Log: simple reconciliation summary — row count from each side.
-- Lets the buyer eyeball whether the two reference files arrived intact
-- before the inventory and pricing imports run.
SELECT
    'pricing'  AS source,
    COUNT(*)   AS row_count
FROM read_xlsx('{{input:Pricing}}')

UNION ALL

SELECT
    'categories' AS source,
    COUNT(*)     AS row_count
FROM read_csv_auto('{{input:Categories}}');
