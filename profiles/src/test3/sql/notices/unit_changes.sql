-- Flags SKUs whose unit of measure differs from a baseline. The baseline is
-- inlined here as a small VALUES set for the mock; in production it would
-- come from a reference table or a second input file.
WITH baseline(sku, unit) AS (
    VALUES
        ('AX-0511', 'EA'),
        ('CX-2204', 'BX'),
        ('DX-1180', 'EA')
)
SELECT
    c."SKU"            AS "SKU",
    b.unit             AS "Previous Unit",
    UPPER(c."Unit")    AS "New Unit"

FROM read_csv_auto('{{input_file}}') c
JOIN baseline b ON b.sku = c."SKU"

WHERE b.unit <> UPPER(c."Unit");
