-- Pricing_Update: reads the vendor pricing workbook and emits one row per
-- SKU with the latest price rounded to two decimals.
SELECT
    TRIM("SKU")                       AS sku,
    ROUND(CAST("Price" AS DOUBLE), 2) AS price

FROM read_xlsx('{{input:Pricing}}')

WHERE "SKU" IS NOT NULL
  AND "Price" IS NOT NULL

ORDER BY sku;
