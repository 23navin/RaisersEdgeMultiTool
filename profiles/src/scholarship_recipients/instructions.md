# Scholarship Recipients

Turns a foundation scholarship award list into an RE import file. The run asks
RE which funds exist, lets you point the projects that match none of them at the
right fund, collects a start and end date for each award term, and writes the
file for the rows that resolved.

---

<!-- label: AddSourceFile -->
## Upload the award list

Attach the award list exactly as the foundation sent it — no re-saving, no
renaming columns. Validate checks the five columns the run depends on:
`BroncoID`, `Award QTR/YR`, `Foundation Project`, `Project Title` and `Amount`.

`Award QTR/YR` is a two-line header cell in the source workbook. That's expected;
the profile matches it as-is.

<!-- label: FetchFunds -->
## Ask RE for the fund list

Pulls every fund id from RE in one query, so the match below happens locally and
can report what *didn't* match. Nothing is written and nothing changes in RE.

Without a connection (or with mock mode on) this reads the bundled fixture
instead, and the badge on the step says which one you got.

<!-- label: FundOverrides -->
## Point the unmatched projects at a fund

Each row here is a `Foundation Project` value with no matching fund id in RE,
shown with the project title it arrived under and how many award rows it covers.
An empty table means every project matched and there's nothing to do.

Where a project is really a fund you recognise under another name, pick that
fund from the dropdown — it lists every fund RE returned, searchable by id or by
description — and those award rows import against it.

Leave a row blank when the fund genuinely doesn't exist yet. **Rows left
unmapped are excluded from everything after this step**: take them back to the
foundation, or have the fund created in RE and re-run the query above.

<!-- label: SemesterDates -->
## Set each term's dates

One row per award term still in play — `F25`, `SP26`, and so on. Terms that
exist only on rows you left unmapped above aren't listed, and pointing a project
at a fund can make a new term appear here. Fill in the start and end date for each one;
they become the `Date From` and `Date To` on every award row carrying that term.

The dates save as you type. A term left blank isn't an error, but its rows won't
reach the import file until it's filled in.

<!-- label: CreateImportFile -->
## Generate the import file

Writes one row per award: the Bronco ID, the amount and project title as a
single `Project Details` field, the dates for that row's term, and the matched
fund id.

If anything was left out, the notice under the button names each row and says
why — a project with no fund chosen, or a term still missing its dates.

<!-- label: Import -->
## Import into RE

1. Download the generated file.
2. In RE, open **Administration → Import** and choose the award/gift import
   type this file is built for.
3. Map the five columns — `Bronco ID`, `Project Details`, `Date From`,
   `Date To`, `Fund ID` — to their RE fields.
4. Run a validation pass first. Import once it comes back clean.
