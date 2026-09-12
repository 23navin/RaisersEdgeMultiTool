// tableScroll.ts
//
// Shared sizing for the result tables in the Imports workspace (validation
// errors, transform errors, notice rows, code-table sync failures). A bad
// vendor file can produce hundreds of rows; past ROW_LIMIT the table scrolls
// inside a fixed box — `.ui-table-scroll` in index.css sizes it to the sticky
// header plus ROW_LIMIT rows — instead of pushing the rest of the step off
// screen.
//
// The stagger reveal is capped at the same count: rows below the fold carry no
// animation class, so they are already there the moment the user scrolls to
// them. Row geometry is pinned by `leading-[16px]` on the table, which is what
// makes the CSS box height land on exactly ROW_LIMIT rows.

import type { CSSProperties } from "react";

export const ROW_LIMIT = 10;

// Classes for the block wrapping a table: a scroll box once the row count
// exceeds the fold, the plain clipped block otherwise.
export function tableBoxClass(rowCount: number) {
  return rowCount > ROW_LIMIT ? "ui-table-scroll" : "overflow-hidden";
}

// Sticky header, but only when the box actually scrolls — otherwise the
// header would stick against the page scroller instead.
export function headerStickyClass(rowCount: number) {
  return rowCount > ROW_LIMIT ? "sticky top-0 z-10" : "";
}

// Staggered reveal for the visible rows; nothing for the rest.
export function rowRevealProps(i: number): {
  className: string;
  style?: CSSProperties;
} {
  return i < ROW_LIMIT
    ? {
        className: "ui-reveal-row",
        style: { animationDelay: `${120 + i * 35}ms` },
      }
    : { className: "" };
}
