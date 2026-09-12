// NoticeBlock.tsx
//
// Informational, non-blocking callout. Used after a successful sql_transform
// (run_profile notices) and after a file_input validation that hit a fuzzy
// column-name match.

import { InfoIcon } from "lucide-react";
import type { Notice } from "../../types";
import { headerStickyClass, rowRevealProps, tableBoxClass } from "./tableScroll";

export function NoticeBlock({ notice }: { notice: Notice }) {
  return (
    <div className="ui-reveal mt-[8px] bg-[#fffbeb] border border-[#fde68a] overflow-hidden">
      <div className="flex items-start gap-[6px] px-[10px] py-[6px] border-b border-[#fde68a] bg-[#fef3c7]">
        <InfoIcon size={13} className="text-[#92400e] mt-[1px] shrink-0" />
        <div className="min-w-0">
          <div className="text-[12px] font-medium text-[#92400e] my-[-1px]">
            {notice.label}
          </div>
          {notice.description && (
            <div className="text-[12px] text-[#a16207] leading-snug mt-[1px]">
              {notice.description}
            </div>
          )}
        </div>
      </div>
      {notice.columns.length > 0 && (
        // Only the rows scroll — the label banner above stays put.
        <div className={tableBoxClass(notice.rows.length)}>
          <table className="w-full text-[12px] leading-[16px] border-collapse">
            <thead
              className={`text-[#a16207] bg-[#fffbeb] ${headerStickyClass(notice.rows.length)}`}
            >
              <tr>
                {notice.columns.map((c) => (
                  <th
                    key={c}
                    className="text-left px-[10px] py-[5px] font-medium"
                  >
                    {c}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {notice.rows.map((row, i) => {
                const reveal = rowRevealProps(i);
                return (
                  <tr
                    key={i}
                    className={`${reveal.className} border-t border-[#fde68a]`}
                    style={reveal.style}
                  >
                    {row.map((cell, j) => (
                      <td
                        key={j}
                        className="px-[10px] py-[5px] font-mono text-[#78350f]"
                      >
                        {cell}
                      </td>
                    ))}
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
