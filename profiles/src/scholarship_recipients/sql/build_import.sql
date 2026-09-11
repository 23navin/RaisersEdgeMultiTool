-- build_import.sql — the import file.
--
-- Joins everything the run has by now: the upload, the fund list RE returned,
-- the project→fund mapping the operator chose, and the dates they typed. A row
-- whose project resolves to no fund — or whose term has no dates yet — is
-- absent here and named by excluded_rows.sql instead.
--
-- Both form placeholders are read with an explicit column list rather than
-- read_json_auto: a form with no rows publishes [], and auto has no schema to
-- infer from an empty array.

WITH src AS (
  -- "Bronco ID" and "Award QTR/YR" are two-line cells in the workbook. The
  -- harness flattens every header's line breaks before the SQL sees the file,
  -- so they are quoted here exactly as they read on screen.
  SELECT
    TRIM(CAST("Bronco ID" AS VARCHAR))          AS bronco_id,
    TRIM(CAST("Award QTR/YR" AS VARCHAR))       AS term,
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
overrides AS (
  SELECT
    UPPER(TRIM(key))                 AS source_key,
    UPPER(TRIM(replacement_fund_id)) AS replacement_key
  FROM read_json('{{form:FundOverrides}}', columns={
    key: 'VARCHAR', replacement_fund_id: 'VARCHAR'
  })
  WHERE replacement_fund_id IS NOT NULL AND TRIM(replacement_fund_id) <> ''
),
-- The one place the fund is decided: the project itself when RE knows it,
-- otherwise whatever the operator mapped it to. The mapped id is re-joined
-- against the fund list rather than trusted, so an override left over from a
-- fund list that has since changed drops the row instead of writing a fund id
-- RE no longer has.
resolved AS (
  SELECT
    s.*,
    COALESCE(direct.fund_id, mapped.fund_id) AS resolved_fund_id
  FROM src s
  LEFT JOIN funds direct ON UPPER(s.fund_id) = direct.fund_key
  LEFT JOIN overrides o  ON UPPER(s.fund_id) = o.source_key
  LEFT JOIN funds mapped ON o.replacement_key = mapped.fund_key
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
  r.bronco_id                                       AS "Bronco ID",
  -- "$1,500.00 : Presidential Scholarship"
  printf('$%,.2f : %s', r.amount, r.project_title)  AS "Project Details",
  -- RE wants US-format dates; the date boxes hand over ISO.
  strftime(t.date_from, '%m/%d/%Y')                 AS "Date From",
  strftime(t.date_to,   '%m/%d/%Y')                 AS "Date To",
  r.resolved_fund_id                                AS "Fund ID"
FROM resolved r
JOIN terms t ON r.term = t.term
WHERE r.resolved_fund_id IS NOT NULL
ORDER BY r.bronco_id, r.resolved_fund_id
