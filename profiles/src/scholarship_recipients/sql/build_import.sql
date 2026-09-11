-- build_import.sql — the import file.
--
-- Joins the three things the run has by now: the upload, the fund list RE
-- returned, and the dates the operator typed. Both joins are inner, so a row
-- whose project isn't a fund — or whose term has no dates yet — is absent here
-- and named by excluded_rows.sql instead.
--
-- {{form:SemesterDates}} is read with an explicit column list rather than
-- read_json_auto: a form with no rows publishes [], and auto has no schema to
-- infer from an empty array.

WITH src AS (
  -- COLUMNS('^Award') picks the "Award QTR/YR" column by prefix — its header is
  -- a two-line cell whose trailing space before the break can't be seen in a
  -- quoted identifier. See semester_terms.sql for the full note.
  SELECT
    TRIM(CAST("BroncoID" AS VARCHAR))           AS bronco_id,
    TRIM(CAST(COLUMNS('^Award') AS VARCHAR))    AS term,
    TRIM(CAST("Foundation Project" AS VARCHAR)) AS fund_id,
    TRIM(CAST("Project Title" AS VARCHAR))      AS project_title,
    TRY_CAST(
      REPLACE(REPLACE(TRIM(CAST("Amount" AS VARCHAR)), '$', ''), ',', '')
      AS DOUBLE
    )                                           AS amount
  FROM read_xlsx('{{input:Recipients}}')
),
funds AS (
  SELECT DISTINCT
    TRIM(CAST(fund_id AS VARCHAR))        AS fund_id,
    UPPER(TRIM(CAST(fund_id AS VARCHAR))) AS fund_key
  FROM read_json_auto('{{query:Funds}}')
),
terms AS (
  SELECT
    TRIM(key)                   AS term,
    TRY_CAST(date_from AS DATE) AS date_from,
    TRY_CAST(date_to   AS DATE) AS date_to
  FROM read_json('{{form:SemesterDates}}', columns={
    key: 'VARCHAR', date_from: 'VARCHAR', date_to: 'VARCHAR'
  })
  WHERE date_from IS NOT NULL AND date_to IS NOT NULL
)
SELECT
  s.bronco_id                                       AS "Bronco ID",
  -- "$1,500.00 : Presidential Scholarship"
  printf('$%,.2f : %s', s.amount, s.project_title)  AS "Project Details",
  -- RE wants US-format dates; the date boxes hand over ISO.
  strftime(t.date_from, '%m/%d/%Y')                 AS "Date From",
  strftime(t.date_to,   '%m/%d/%Y')                 AS "Date To",
  f.fund_id                                         AS "Fund ID"
FROM src s
JOIN funds f ON UPPER(s.fund_id) = f.fund_key
JOIN terms t ON s.term = t.term
ORDER BY s.bronco_id, f.fund_id
