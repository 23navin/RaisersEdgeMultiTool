-- excluded_rows.sql — notice on the CreateImportFile step.
--
-- The mirror image of build_import.sql: every award row the two inner joins
-- dropped, with the reason. Returns nothing when the file is complete.

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
terms AS (
  SELECT TRIM(key) AS term
  FROM read_json('{{form:SemesterDates}}', columns={
    key: 'VARCHAR', date_from: 'VARCHAR', date_to: 'VARCHAR'
  })
  WHERE date_from IS NOT NULL AND date_from <> ''
    AND date_to   IS NOT NULL AND date_to   <> ''
)
SELECT
  s.bronco_id     AS "Bronco ID",
  s.fund_id       AS "Foundation Project",
  s.project_title AS "Project Title",
  s.term          AS "Award QTR/YR",
  CASE
    WHEN f.fund_key IS NULL AND t.term IS NULL
      THEN 'No matching Fund ID, and no dates for this term'
    WHEN f.fund_key IS NULL
      THEN 'No matching Fund ID in RE'
    ELSE 'No start/end date filled in for this term'
  END             AS "Why it was left out"
FROM src s
LEFT JOIN funds f ON UPPER(s.fund_id) = f.fund_key
LEFT JOIN terms t ON s.term = t.term
WHERE f.fund_key IS NULL OR t.term IS NULL
ORDER BY s.fund_id, s.bronco_id
