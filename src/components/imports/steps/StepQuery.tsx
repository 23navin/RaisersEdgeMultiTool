// StepQuery.tsx
//
// Renders steps with type=re_query.
// The step sends values from the uploaded files to RE and stores the rows it
// gets back; a later sql_transform reads them via {{query:Label}}. Unlike a
// transform there's no output file and nothing to download — the result is a
// row count plus the label downstream SQL will use.
//
// The RE query flow (execute -> poll job -> download) takes seconds to minutes,
// so the running state is indeterminate rather than a percentage: the backend
// polls at a fixed interval and cannot report real progress.

import {
  FileTextIcon,
  DatabaseIcon,
  DownloadCloudIcon,
  CheckIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "../../ui/button";
import { cn } from "../../../lib/utils";
import type { QueryStepResult } from "../../../types";

export type QueryStatus = "idle" | "running" | "done" | "error";

export type QueryRow = {
  queryOutput: string;   // label downstream SQL reads
  inputs: { label: string; ready: boolean }[];
  canRun: boolean;
  status: QueryStatus;
  result?: QueryStepResult;
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

export function StepQuery({
  description,
  query,
}: {
  description?: string;
  query: QueryRow;
}) {
  const disabled = !query.canRun || query.status === "running";
  const btnStyle =
    query.status === "done" ? doneBtn : query.canRun ? activeBtn : notReadyBtn;
  const r = query.result;

  return (
    <div className="border border-[#e5e2dc] bg-white px-[18px] py-[16px]">
      {description && (
        <p className="text-[13px] text-neutral-600 mb-[12px] whitespace-pre-line">
          {description}
        </p>
      )}

      {/* inputs → RE → named result */}
      <div className="flex items-center w-full mb-[10px]">
        <div className="flex-1 flex items-center justify-end min-w-0">
          <div className="flex flex-col gap-[5px] shrink-0">
            {query.inputs.length > 0 ? (
              query.inputs.map((item) => (
                <PipeNode
                  key={item.label}
                  icon={FileTextIcon}
                  label={item.label}
                  ready={item.ready}
                />
              ))
            ) : (
              <span className="px-[10px] py-[4px] text-[11px] text-neutral-400">
                no inputs
              </span>
            )}
          </div>
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
        </div>

        <div className="shrink-0 px-[4px]">
          <DownloadCloudIcon size={22} className="text-neutral-400" />
        </div>

        <div className="flex-1 flex items-center justify-start min-w-0">
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
          <div className="flex flex-col gap-[5px] shrink-0">
            <PipeNode
              icon={DatabaseIcon}
              label={query.queryOutput}
              ready={query.status === "done"}
            />
          </div>
        </div>
      </div>

      <div className="flex items-center gap-[8px]">
        <Button
          type="button"
          onClick={query.onRun}
          disabled={disabled}
          className={`${baseBtn} ${btnStyle}`}
        >
          <DownloadCloudIcon size={13} />
          {query.status === "running" ? "Querying RE…" : "Run Query"}
        </Button>

        {/* Indeterminate bar — the SKY job gives no progress to report. */}
        <div className="flex-1 h-[5px] bg-neutral-200 overflow-hidden">
          {query.status === "running" ? (
            <div className="h-full w-1/3 bg-neutral-400 animate-pulse" />
          ) : (
            <div
              className={cn(
                "h-full transition-[width] duration-200",
                query.status === "done"
                  ? "w-full bg-green-500"
                  : query.status === "error"
                  ? "w-full bg-red-500"
                  : "w-0",
              )}
            />
          )}
        </div>

        {r && query.status !== "running" && (
          <span
            className={cn(
              "px-[5px] py-[1px] rounded text-[9px] uppercase tracking-[0.06em] font-medium",
              r.mode === "live"
                ? "bg-green-100 text-green-700"
                : "bg-neutral-100 text-neutral-500",
            )}
            title={
              r.mode === "live"
                ? "Live data from Raiser's Edge NXT"
                : "Mock data (fixtures) — connect RE NXT in Settings for live data"
            }
          >
            {r.mode}
          </span>
        )}
      </div>

      {query.status === "done" && r && (
        <p className="mt-[10px] text-[12px] text-neutral-600">
          {r.row_count} row{r.row_count === 1 ? "" : "s"} returned — available to
          later steps as{" "}
          <code className="px-[4px] py-[1px] bg-neutral-100 text-[11px]">
            {`{{query:${r.query_output}}}`}
          </code>
        </p>
      )}

      {query.status === "error" && query.error && (
        <div className="mt-[12px] border border-red-200 bg-red-50 px-[12px] py-[9px] text-[12px] text-red-800">
          {query.error}
        </div>
      )}
    </div>
  );
}
