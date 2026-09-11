-- fund_choices.sql — options_sql for the FundOverrides step's select field.
--
-- Every fund RE returned, offered as the replacement choices. First column is
-- the value stored in the form (and joined on downstream); the second is what
-- the operator reads in the dropdown, so a fund is findable by id or by name.
--
-- Because the list comes from the same {{query:Funds}} the matching uses, a
-- chosen replacement is always a real fund — there is no free-text path here
-- for a typo to take.

SELECT
  TRIM(CAST(fund_id AS VARCHAR))                                       AS value,
  TRIM(CAST(fund_id AS VARCHAR)) || ' — ' ||
    COALESCE(TRIM(CAST(fund_description AS VARCHAR)), '(no description)') AS label
FROM read_json_auto('{{query:Funds}}')
WHERE fund_id IS NOT NULL AND TRIM(CAST(fund_id AS VARCHAR)) <> ''
ORDER BY value
