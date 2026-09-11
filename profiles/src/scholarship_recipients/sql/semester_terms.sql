-- semester_terms.sql — rows_sql for the SemesterDates step.
--
-- One row per award term still in play: the distinct "Award QTR/YR" values
-- (F25, SP26, …) on rows that resolve to a fund — either because the
-- Foundation Project is a fund id in RE, or because the previous step mapped it
-- to one. Terms that appear only on rows with neither are left out; those rows
-- never reach the import file, so there is nothing to date.
--
-- That dependency on the override form is why this step declares
-- `form_input: [FundOverrides]` and comes after it: mapping a project can bring
-- a whole new term into play, and this list has to grow when it does. The
-- date fields are required, so listing a term that can never import would
-- block the run instead of just cluttering it.

WITH src AS (
  -- The real header is a two-line cell — "Award", a newline, a space, then
  -- "QTR/YR" — which no quoted identifier could match. Every input file is
  -- read through the harness's flattened-header projection (runs of whitespace
  -- collapsed to one space, ends trimmed), so the name below is what the SQL
  -- sees no matter how the export's whitespace shifts.
  -- all_varchar=true is what keeps the fund match working. A Foundation Project
  -- like 946117 is a *number* in the workbook, so DuckDB types the column DOUBLE
  -- and CAST(... AS VARCHAR) renders it "946117.0" — which matches no fund id RE
  -- ever returns. Reading every cell as text gives back exactly what the cell
  -- shows, and preserves any leading zeros an id column carries.
  SELECT
    TRIM(CAST("Award QTR/YR" AS VARCHAR))       AS term,
    TRIM(CAST("Foundation Project" AS VARCHAR)) AS fund_id
  FROM read_xlsx('{{input:Recipients}}', all_varchar=true)
),
funds AS (
  SELECT DISTINCT UPPER(TRIM(CAST(fund_id AS VARCHAR))) AS fund_key
  FROM read_json_auto('{{query:Funds}}')
),
overrides AS (
  -- key = the Foundation Project the operator mapped; replacement_fund_id =
  -- the fund they picked. Read with an explicit column list: a form with no
  -- rows publishes [], which read_json_auto has no schema to infer from.
  SELECT
    UPPER(TRIM(key))                 AS source_key,
    UPPER(TRIM(replacement_fund_id)) AS replacement_key
  FROM read_json('{{form:FundOverrides}}', columns={
    key: 'VARCHAR', replacement_fund_id: 'VARCHAR'
  })
  WHERE replacement_fund_id IS NOT NULL AND TRIM(replacement_fund_id) <> ''
)
SELECT DISTINCT s.term
FROM src s
LEFT JOIN funds  direct ON UPPER(s.fund_id) = direct.fund_key
LEFT JOIN overrides o   ON UPPER(s.fund_id) = o.source_key
LEFT JOIN funds  mapped ON o.replacement_key = mapped.fund_key
WHERE s.term IS NOT NULL AND s.term <> ''
  AND COALESCE(direct.fund_key, mapped.fund_key) IS NOT NULL
ORDER BY s.term
