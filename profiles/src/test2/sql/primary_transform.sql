SELECT
    CAST("ID" AS INTEGER)       AS id,
    TRIM("name")                AS name,
    "class"                     AS class,
    COALESCE("subclass", '')    AS subclass

FROM read_csv_auto('{{input_file}}')

WHERE "class" IN ('Alpha', 'Beta', 'Gamma')
  AND "ID" IS NOT NULL

ORDER BY "ID";