-- Surfaces category values that aren't in the recognized list.
-- Returns one row per offending input row. Empty result = nothing to flag.
-- The "Row" column is 1-based from the input file (header counts as row 1).
SELECT
    ROW_NUMBER() OVER () + 1   AS "Row",
    "SKU"                      AS "SKU",
    "Category"                 AS "Category"

FROM read_csv_auto('{{input_file}}')

WHERE "Category" NOT IN (
    'Hardware', 'Plumbing', 'Electrical', 'Paint', 'Tools', 'Garden'
);
