-- unmatched_projects.sql — rows_sql for the FundOverrides step.
--
-- One row per Foundation Project in the upload that is not itself a fund id in
-- RE, with the project title it arrived under and how many award rows it
-- covers. Each row gets a dropdown for picking the fund to use instead.
--
-- Deliberately does NOT read {{form:FundOverrides}}: these are the rows the
-- form asks about, so a project stays listed after it has been mapped — that
-- is how the operator sees and changes the choice they made.
--
-- The step keys rows on "Foundation Project" (structure.yaml's key_column), so
-- that value is what downstream SQL joins on as `key`.

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
