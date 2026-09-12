-- excluded_rows.sql — notice on the GenerateImportFile step.
--
-- The mirror image of build_import.sql: every Clean Roster row that didn't make
-- it, with the reason. Twenty-four rows, so the notice table scrolls.

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
  s.record_id   AS "Record ID",
  s.fund_id     AS "Fund ID",
  s.gift_date   AS "Gift Date",
  s.amount_text AS "Amount",
  CASE
    WHEN s.status <> 'Active'
      THEN 'Status is ' || s.status
    WHEN f.fund_id IS NULL
      THEN 'Fund ID is not one of the three open funds'
    WHEN TRY_CAST(s.amount_text AS DOUBLE) <= 0
      THEN 'Amount is zero or negative'
    ELSE 'Gift date is outside FY26'
  END           AS "Why it was left out"
FROM src s
LEFT JOIN funds f ON s.fund_id = f.fund_id
WHERE s.status <> 'Active'
   OR f.fund_id IS NULL
   OR TRY_CAST(s.amount_text AS DOUBLE) <= 0
   OR s.gift_date NOT BETWEEN '2025-07-01' AND '2026-06-30'
ORDER BY s.record_id
