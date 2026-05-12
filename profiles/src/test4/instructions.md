# Order Export

Splits the vendor's order export into three files — one per status — using a
single SQL transform that writes all three outputs in one batch.

---

<!-- label: UploadOrders -->
## Upload Orders

Drop the vendor's `orders.csv` export. Required columns: `Order #` (8 digits),
`Status` (one of `Shipped`, `Pending`, `Cancelled`).

<!-- label: SplitOrders -->
## Split by Status

Produces `Shipped_Orders`, `Pending_Orders`, and `Cancelled_Orders` in a single
run. Each Download button below saves one of the three files.

<!-- label: Import -->
## Import into database

Load each file into its matching staging table using the `OrderImport` profile.
