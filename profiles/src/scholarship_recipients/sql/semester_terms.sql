-- semester_terms.sql — rows_sql for the SemesterDates step.
--
-- One row per award term still in play: the distinct "Award QTR/YR" values
-- (F25, SP26, …) on rows whose Foundation Project matches a fund in RE. Terms
-- that appear only on unmatched rows are left out — those rows never reach the
-- import file, so there is nothing to date.
--
-- The step draws one start/end date pair per row returned here, keyed on
-- `term` (structure.yaml's key_column).

WITH src AS (
  -- COLUMNS('^Award') picks the "Award QTR/YR" column by prefix. Its real
  -- header is a two-line cell — "Award ", a newline, then "QTR/YR" — and the
  -- trailing space before the break is invisible in a quoted identifier, so
  -- matching on the prefix is what keeps this working when the export's
  -- whitespace shifts. It fails loudly if a second Award* column ever appears.
  SELECT
    TRIM(CAST(COLUMNS('^Award') AS VARCHAR))    AS term,
    TRIM(CAST("Foundation Project" AS VARCHAR)) AS fund_id
  FROM read_xlsx('{{input:Recipients}}')
),
funds AS (
  SELECT DISTINCT UPPER(TRIM(CAST(fund_id AS VARCHAR))) AS fund_key
  FROM read_json_auto('{{query:Funds}}')
)
SELECT DISTINCT s.term
FROM src s
JOIN funds f ON UPPER(s.fund_id) = f.fund_key
WHERE s.term IS NOT NULL AND s.term <> ''
ORDER BY s.term
