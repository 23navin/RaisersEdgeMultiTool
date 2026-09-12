-- build_import.sql — the Clean Roster rows that belong in the import file.
--
-- A row makes it in only when it is Active, carries a positive amount, falls
-- inside FY26, and names one of the three open funds. Six of the thirty rows
-- do; excluded_rows.sql lists the other twenty-four as a notice on this step,
-- which is what makes that notice long enough to scroll.

WITH src AS (
  SELECT
    TRIM(CAST("Record ID" AS VARCHAR)) AS record_id,
    TRIM(CAST("Fund ID"   AS VARCHAR)) AS fund_id,
    TRIM(CAST("Gift Date" AS VARCHAR)) AS gift_date,
    TRIM(CAST("Amount"    AS VARCHAR)) AS amount_text,
    TRIM(CAST("Status"    AS VARCHAR)) AS status
  FROM read_csv_auto('{{input:Clean Roster}}')
),
funds AS (
  SELECT * FROM (VALUES ('104530'), ('104531'), ('104532')) AS t(fund_id)
)
SELECT
  s.record_id                       AS constituent_id,
  s.fund_id                         AS fund_id,
  s.gift_date                       AS gift_date,
  TRY_CAST(s.amount_text AS DOUBLE) AS amount,
  'Annual Fund'                     AS campaign
FROM src s
JOIN funds f ON s.fund_id = f.fund_id
WHERE s.status = 'Active'
  AND TRY_CAST(s.amount_text AS DOUBLE) > 0
  AND s.gift_date BETWEEN '2025-07-01' AND '2026-06-30'
ORDER BY s.record_id
