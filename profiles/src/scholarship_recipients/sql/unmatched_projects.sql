-- unmatched_projects.sql — the ReviewUnmatched visualization.
--
-- Every Foundation Project in the upload that is not a fund id in RE, with the
-- project title it was sent under and how many award rows it covers. These rows
-- are the ones the import file below leaves out.

WITH src AS (
  SELECT
    TRIM(CAST("Foundation Project" AS VARCHAR)) AS fund_id,
    TRIM(CAST("Project Title" AS VARCHAR))      AS project_title
  FROM read_xlsx('{{input:Recipients}}')
),
funds AS (
  SELECT DISTINCT UPPER(TRIM(CAST(fund_id AS VARCHAR))) AS fund_key
  FROM read_json_auto('{{query:Funds}}')
)
SELECT
  s.fund_id       AS "Foundation Project",
  s.project_title AS "Project Title",
  COUNT(*)        AS "Award rows"
FROM src s
LEFT JOIN funds f ON UPPER(s.fund_id) = f.fund_key
WHERE f.fund_key IS NULL
GROUP BY s.fund_id, s.project_title
ORDER BY s.fund_id, s.project_title
