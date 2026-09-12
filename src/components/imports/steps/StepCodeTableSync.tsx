// StepCodeTableSync.tsx
//
// Renders steps with type=code_table_sync.
// The step's SQL selects rows; each row becomes one create / update / delete
// against an RE code table. Unlike a sql_transform there is no output file —
// the result is a count plus a per-row failure list, so the layout is a
// pipeline diagram (inputs → RE table) and a run row, then a result block.
//
// Writes reach RE only when connected and RE_NXT_MOCK is unset; otherwise the
// backend stubs them and reports mode "mock". The badge makes that visible
// before the user clicks, because this step changes data in RE.

import {
  FileTextIcon,
  TableIcon,
  UploadCloudIcon,
  CheckIcon,
  XIcon,
  AlertTriangleIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "../../ui/button";
import { cn } from "../../../lib/utils";
import { headerStickyClass, tableBoxClass } from "../../shared/tableScroll";
import type { SyncResult } from "../../../types";

export type SyncStatus = "idle" | "running" | "done" | "error";

export type SyncRow = {
  operation: string;       // "create" | "update" | "delete"
  codeTable: string;       // name or id, as declared
  inputs: { label: string; ready: boolean }[];
  canRun: boolean;
  status: SyncStatus;
  result?: SyncResult;
  error?: string;
  onRun: () => void;
};

const baseBtn =
  "rounded-none h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none transform-gpu";
const activeBtn = "bg-[#1a1a1a] hover:bg-[#2a2a2a] text-white cursor-pointer";
const doneBtn =
  "bg-[#e0ddd8] hover:bg-[#d4d0c9] text-neutral-500 hover:text-neutral-700 disabled:opacity-100 cursor-pointer";
const notReadyBtn =
  "bg-neutral-200 hover:bg-neutral-200 text-neutral-400 disabled:opacity-100 cursor-not-allowed";

// Deleting is not recoverable from here, so it never borrows the neutral
// styling the other two use.
const VERB_LABEL: Record<string, string> = {
  create: "Add entries",
  update: "Update entries",
  delete: "Delete entries",
};

function PipeNode({
  icon: Icon,
  label,
  ready,
}: {
  icon: LucideIcon;
  label: string;
  ready: boolean;
}) {
  const StatusIcon = ready ? CheckIcon : XIcon;
  return (
    <div className="inline-flex items-center gap-[5px] px-[10px] py-[4px] text-neutral-500 text-[11px] whitespace-nowrap">
      <Icon size={12} />
      <span className="overflow-hidden text-ellipsis">{label}</span>
      <StatusIcon
        size={12}
        className={ready ? "text-green-600" : "text-red-600"}
        strokeWidth={2.5}
      />
    </div>
  );
}

function ArrowSVG() {
  return (
    <div className="flex items-center w-full">
      <div className="flex-1 h-px bg-[#d1d5db]" />
      <svg width="9" height="12" viewBox="0 0 9 12" className="shrink-0">
        <polygon points="0,2 9,6 0,10" fill="#d1d5db" />
      </svg>
    </div>
  );
}

export function StepCodeTableSync({
  description,
  sync,
}: {
  description?: string;
  sync: SyncRow;
}) {
  const disabled = !sync.canRun || sync.status === "running";
  const btnStyle =
    sync.status === "done" ? doneBtn : sync.canRun ? activeBtn : notReadyBtn;
  const r = sync.result;

  return (
    <div className="border border-[#e5e2dc] bg-white px-[18px] py-[16px]">
      {description && (
        <p className="text-[13px] text-neutral-600 mb-[12px] whitespace-pre-line">
          {description}
        </p>
      )}

      {/* inputs → RE code table */}
      <div className="flex items-center w-full mb-[10px]">
        <div className="flex-1 flex items-center justify-end min-w-0">
          <div className="flex flex-col gap-[5px] shrink-0">
            {sync.inputs.map((item) => (
              <PipeNode
                key={item.label}
                icon={FileTextIcon}
                label={item.label}
                ready={item.ready}
              />
            ))}
          </div>
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
        </div>

        <div className="shrink-0 px-[4px]">
          <UploadCloudIcon size={22} className="text-neutral-400" />
        </div>

        <div className="flex-1 flex items-center justify-start min-w-0">
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
          <div className="flex flex-col gap-[5px] shrink-0">
            <PipeNode
              icon={TableIcon}
              label={sync.codeTable}
              ready={sync.status === "done" && (r?.ok ?? false)}
            />
          </div>
        </div>
      </div>

      <div className="flex items-center gap-[8px]">
        <Button
          type="button"
          onClick={sync.onRun}
          disabled={disabled}
          className={`${baseBtn} ${btnStyle}`}
        >
          <UploadCloudIcon size={13} />
          {sync.status === "running"
            ? "Working…"
            : VERB_LABEL[sync.operation] ?? "Run"}
        </Button>

        <span className="text-[11px] text-neutral-400 flex-1">
          {sync.operation === "delete"
            ? "Removes matching entries from the RE code table."
            : `Writes to the "${sync.codeTable}" code table in RE.`}
        </span>

        {r && sync.status !== "running" && (
          <span
            className={cn(
              "px-[5px] py-[1px] rounded text-[9px] uppercase tracking-[0.06em] font-medium",
              r.mode === "live"
                ? "bg-green-100 text-green-700"
                : "bg-neutral-100 text-neutral-500",
            )}
            title={
              r.mode === "live"
                ? "Written to Raiser's Edge NXT"
                : "Mock mode — nothing was written. Connect RE NXT in Settings."
            }
          >
            {r.mode}
          </span>
        )}
      </div>

      {/* Result */}
      {sync.status === "error" && sync.error && (
        <div className="mt-[12px] border border-red-200 bg-red-50 px-[12px] py-[9px] text-[12px] text-red-800">
          {sync.error}
        </div>
      )}

      {r && sync.status === "done" && (
        <div className="mt-[12px]">
          <div
            className={cn(
              "px-[12px] py-[9px] text-[12px] border",
              r.ok
                ? "border-green-200 bg-green-50 text-green-800"
                : "border-amber-200 bg-amber-50 text-amber-900",
            )}
          >
            {r.message}
          </div>

          {r.sync_output && (
            <p className="mt-[10px] text-[12px] text-neutral-600">
              Outcome rows available to later steps as{" "}
              <code className="px-[4px] py-[1px] bg-neutral-100 text-[11px]">
                {`{{sync:${r.sync_output}}}`}
              </code>
            </p>
          )}

          {r.failures.length > 0 && (
            <div className={`mt-[10px] ${tableBoxClass(r.failures.length)}`}>
              <table className="w-full text-[11px] leading-[16px] border-collapse">
                <thead
                  className={`bg-white ${headerStickyClass(r.failures.length)}`}
                >
                  <tr className="text-left text-neutral-500">
                    <th className="border-b border-[#e5e2dc] py-[5px] pr-[8px] font-medium w-[50px]">
                      Row
                    </th>
                    <th className="border-b border-[#e5e2dc] py-[5px] pr-[8px] font-medium w-[30%]">
                      Entry
                    </th>
                    <th className="border-b border-[#e5e2dc] py-[5px] font-medium">
                      Error
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {r.failures.map((f) => (
                    <tr key={f.row} className="align-top">
                      <td className="border-b border-[#f0eeea] py-[5px] pr-[8px] text-neutral-500">
                        {f.row}
                      </td>
                      <td className="border-b border-[#f0eeea] py-[5px] pr-[8px]">
                        {f.identifier}
                      </td>
                      <td className="border-b border-[#f0eeea] py-[5px] text-neutral-600">
                        <span className="inline-flex items-start gap-[5px]">
                          <AlertTriangleIcon
                            size={11}
                            className="text-amber-600 mt-[2px] shrink-0"
                          />
                          {f.error}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
