-- excluded_rows.sql — notice on the CreateImportFile step.
--
-- The mirror image of build_import.sql: every award row that didn't make it,
-- with the reason. Returns nothing when the file is complete.

WITH src AS (
  SELECT
    TRIM(CAST("BroncoID" AS VARCHAR))           AS bronco_id,
    TRIM(CAST(COLUMNS('^Award') AS VARCHAR))    AS term,
    TRIM(CAST("Foundation Project" AS VARCHAR)) AS fund_id,
    TRIM(CAST("Project Title" AS VARCHAR))      AS project_title
  FROM read_xlsx('{{input:Recipients}}')
),
funds AS (
  SELECT DISTINCT UPPER(TRIM(CAST(fund_id AS VARCHAR))) AS fund_key
  FROM read_json_auto('{{query:Funds}}')
),
overrides AS (
  SELECT
    UPPER(TRIM(key))                 AS source_key,
    UPPER(TRIM(replacement_fund_id)) AS replacement_key
  FROM read_json('{{form:FundOverrides}}', columns={
    key: 'VARCHAR', replacement_fund_id: 'VARCHAR'
  })
  WHERE replacement_fund_id IS NOT NULL AND TRIM(replacement_fund_id) <> ''
),
resolved AS (
  SELECT
    s.*,
    COALESCE(direct.fund_key, mapped.fund_key) AS resolved_fund_key
  FROM src s
  LEFT JOIN funds direct ON UPPER(s.fund_id) = direct.fund_key
  LEFT JOIN overrides o  ON UPPER(s.fund_id) = o.source_key
  LEFT JOIN funds mapped ON o.replacement_key = mapped.fund_key
),
terms AS (
  SELECT TRIM(key) AS term
  FROM read_json('{{form:SemesterDates}}', columns={
    key: 'VARCHAR', date_from: 'VARCHAR', date_to: 'VARCHAR'
  })
  WHERE date_from IS NOT NULL AND TRIM(date_from) <> ''
    AND date_to   IS NOT NULL AND TRIM(date_to)   <> ''
)
SELECT
  r.bronco_id     AS "Bronco ID",
  r.fund_id       AS "Foundation Project",
  r.project_title AS "Project Title",
  r.term          AS "Award QTR/YR",
  CASE
    WHEN r.resolved_fund_key IS NULL AND t.term IS NULL
      THEN 'No fund chosen for this project, and no dates for this term'
    WHEN r.resolved_fund_key IS NULL
      THEN 'Not a Fund ID in RE, and no replacement fund chosen'
    ELSE 'No start/end date filled in for this term'
  END             AS "Why it was left out"
FROM resolved r
LEFT JOIN terms t ON r.term = t.term
WHERE r.resolved_fund_key IS NULL OR t.term IS NULL
ORDER BY r.fund_id, r.bronco_id
