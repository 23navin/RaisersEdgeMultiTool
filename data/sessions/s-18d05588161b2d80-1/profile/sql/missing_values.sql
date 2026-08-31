-- The "table of unavailable options" the user actually reads.
--
-- Runs as a notice on the CheckCodeTable step, so its column aliases are the
-- table headers shown in the UI — plain language, not API field names. A notice
-- returning zero rows renders nothing, which is the nominal case once every
-- code exists in RE.
SELECT
    trim(c."constituent_code") AS "Code In File",
    count(*)                   AS "Rows Affected",
    min(c."constituent_id")    AS "First Record Id"
FROM read_csv_auto('{{input:Constituents}}') c
WHERE trim(coalesce(c."constituent_code", '')) <> ''
  AND lower(trim(c."constituent_code")) NOT IN (
      SELECT lower(trim(long_description))
      FROM read_json_auto('{{codetable:ConstituentCodes}}')
      -- A NULL inside a NOT IN list makes the whole predicate NULL, which would
      -- silently report zero missing codes. Drop them first.
      WHERE long_description IS NOT NULL
  )
GROUP BY 1
ORDER BY "Rows Affected" DESC, "Code In File";
