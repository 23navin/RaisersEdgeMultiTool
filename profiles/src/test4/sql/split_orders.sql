-- Splits the order export into three files by Status. Uses multi-output mode:
-- the {{output:Label}} placeholders resolve to per-output temp paths, and the
-- author writes the COPY statements themselves. The backend executes the
-- batch as-is (see db.rs run_transform multi-output branch).

COPY (
    SELECT *
    FROM read_csv_auto('{{input_file}}')
    WHERE TRIM("Status") = 'Shipped'
) TO '{{output:Shipped_Orders}}' (HEADER, DELIMITER ',');

COPY (
    SELECT *
    FROM read_csv_auto('{{input_file}}')
    WHERE TRIM("Status") = 'Pending'
) TO '{{output:Pending_Orders}}' (HEADER, DELIMITER ',');

COPY (
    SELECT *
    FROM read_csv_auto('{{input_file}}')
    WHERE TRIM("Status") = 'Cancelled'
) TO '{{output:Cancelled_Orders}}' (HEADER, DELIMITER ',');
