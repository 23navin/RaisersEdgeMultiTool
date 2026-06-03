-- ConstituentGifts: flattens the RE query's gift rows for the table viz.
-- The query result is written to the temp dir as JSON and read here via the
-- {{query:Label}} placeholder + DuckDB's read_json_auto. Columns selected here
-- must match the `field` names declared in the visualization's config.
SELECT
    "constituent_name"   AS constituent_name,
    "gift_id"            AS gift_id,
    CAST("gift_amount" AS DECIMAL(18,2)) AS gift_amount,
    "gift_date"         AS gift_date

FROM read_json_auto('{{query:GiftRows}}')

ORDER BY "gift_date" DESC;
