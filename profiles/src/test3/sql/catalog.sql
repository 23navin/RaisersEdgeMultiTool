SELECT
    TRIM("SKU")       AS sku,
    TRIM("Category")  AS category,
    UPPER("Unit")     AS unit

FROM read_csv_auto('{{input_file}}')

WHERE "SKU" IS NOT NULL

ORDER BY sku;
