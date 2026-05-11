SELECT
    CAST("ID" AS INTEGER)        AS id,
    TRIM("name")                 AS name,
    "class"                      AS class,
    CASE "class"
        WHEN 'Alpha' THEN 1
        WHEN 'Beta'  THEN 2
        WHEN 'Gamma' THEN 3
    END                          AS priority,
    COALESCE("subclass", 'None') AS subclass

FROM read_csv_auto('{{input_file}}')

WHERE "class" IN ('Alpha', 'Beta', 'Gamma')
  AND "ID" IS NOT NULL

ORDER BY priority, name;
