// TableViz.tsx
//
// Renders a ResultSet as an interactive table. Hand-rolled for the vertical;
// the eventual target is TanStack Table (the registry seam keeps that swap
// contained). Honors a `config` of the shape:
//   { sortable?: boolean, columns?: { field, header?, format? }[] }
// where `field` matches a ResultSet column name. With no `columns`, every
// column is shown in order.

import { useMemo, useState } from "react";
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react";
import type { VizProps } from "./types";

type ColumnSpec = { field: string; header?: string; format?: string };
type TableConfig = { sortable?: boolean; columns?: ColumnSpec[] };

function formatCell(value: string, format?: string): string {
  if (value === "" || value == null) return "";
  if (format === "currency") {
    const n = Number(value);
    return Number.isFinite(n)
      ? n.toLocaleString(undefined, { style: "currency", currency: "USD" })
      : value;
  }
  return value;
}

// Numeric when both parse as finite numbers, else case-insensitive string.
function compareValues(a: string, b: string): number {
  const na = Number(a);
  const nb = Number(b);
  if (Number.isFinite(na) && Number.isFinite(nb)) return na - nb;
  return a.localeCompare(b, undefined, { sensitivity: "base" });
}

export function TableViz({ data, config }: VizProps) {
  const cfg = (config ?? {}) as TableConfig;

  // Resolve which ResultSet column index each displayed column maps to.
  const cols = useMemo(() => {
    const specs: ColumnSpec[] =
      cfg.columns && cfg.columns.length > 0
        ? cfg.columns
        : data.columns.map((field) => ({ field }));
    return specs
      .map((spec) => ({ spec, idx: data.columns.indexOf(spec.field) }))
      .filter((c) => c.idx !== -1);
  }, [cfg.columns, data.columns]);

  const [sort, setSort] = useState<{ idx: number; dir: 1 | -1 } | null>(null);
  const sortable = cfg.sortable ?? false;

  const rows = useMemo(() => {
    if (!sort) return data.rows;
    const sorted = [...data.rows].sort(
      (ra, rb) => compareValues(ra[sort.idx] ?? "", rb[sort.idx] ?? "") * sort.dir,
    );
    return sorted;
  }, [data.rows, sort]);

  const toggleSort = (idx: number) => {
    if (!sortable) return;
    setSort((prev) =>
      prev && prev.idx === idx
        ? { idx, dir: prev.dir === 1 ? -1 : 1 }
        : { idx, dir: 1 },
    );
  };

  if (data.rows.length === 0) {
    return (
      <div className="px-[14px] py-[12px] text-[13px] text-neutral-400">
        No rows.
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[13px] border-collapse">
        <thead>
          <tr className="border-b border-neutral-200">
            {cols.map(({ spec, idx }) => {
              const active = sort?.idx === idx;
              return (
                <th
                  key={spec.field}
                  onClick={() => toggleSort(idx)}
                  className={
                    "text-left font-medium text-neutral-600 px-[10px] py-[7px] whitespace-nowrap " +
                    (sortable ? "cursor-pointer select-none hover:text-neutral-900" : "")
                  }
                >
                  <span className="inline-flex items-center gap-[3px]">
                    {spec.header ?? spec.field}
                    {active &&
                      (sort!.dir === 1 ? (
                        <ChevronUpIcon size={12} />
                      ) : (
                        <ChevronDownIcon size={12} />
                      ))}
                  </span>
                </th>
              );
            })}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, r) => (
            <tr
              key={r}
              className="border-b border-neutral-100 hover:bg-neutral-50"
            >
              {cols.map(({ spec, idx }) => (
                <td
                  key={spec.field}
                  className="px-[10px] py-[6px] text-neutral-800 whitespace-nowrap"
                >
                  {formatCell(row[idx] ?? "", spec.format)}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
