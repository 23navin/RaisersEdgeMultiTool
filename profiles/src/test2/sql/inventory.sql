-- Inventory_Import: joins the vendor's inventory export with the Categories
-- lookup table and stamps a normalized priority. Both inputs must be
-- uploaded — if Categories is skipped the SQL fails with an unsubstituted
-- {{input:Categories}} placeholder.
SELECT
    CAST(i."Item #" AS INTEGER)                  AS item_id,
    TRIM(i."Category")                           AS category,
    COALESCE(c."Description", i."Category")      AS category_description,
    CASE TRIM(i."Category")
        WHEN 'Alpha' THEN 1
        WHEN 'Beta'  THEN 2
        WHEN 'Gamma' THEN 3
    END                                          AS priority

FROM read_csv_auto('{{input:Inventory}}')        i
LEFT JOIN read_csv_auto('{{input:Categories}}')  c
       ON c."Category" = i."Category"

WHERE i."Item #" IS NOT NULL
  AND i."Category" IN ('Alpha', 'Beta', 'Gamma')

ORDER BY priority, item_id;
