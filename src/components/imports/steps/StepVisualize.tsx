// StepVisualize.tsx
//
// Renders steps with type=visualization.
// The step reads what earlier steps already produced — uploaded files, an
// re_query result, a code_table_sync outcome — and draws it on screen. Nothing
// is written and nothing is sent to RE, so there is no output file, no download
// and no live/mock badge: the data is already local by the time this runs.
//
// The drawing itself is delegated to the report VIZ_REGISTRY, so a table here
// and a table on the Reports tab are the same component.

import { BarChart3Icon, FileTextIcon, DatabaseIcon, CheckIcon, XIcon, type LucideIcon } from "lucide-react";
import { Button } from "../../ui/button";
import { cn } from "../../../lib/utils";
import { VIZ_REGISTRY } from "../../reports/viz";
import type { ResultSet, StepVisualization } from "../../../types";

export type VizStatus = "idle" | "running" | "done" | "error";

// One upstream source feeding the step. `kind` picks the icon: a file the user
// uploaded vs. a named result an earlier step published.
export type VizSource = {
  label: string;
  ready: boolean;
  kind: "file" | "query" | "sync";
};

export type VizRow = {
  spec: StepVisualization;
  sources: VizSource[];
  canRun: boolean;
  status: VizStatus;
  data?: ResultSet;
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

const SOURCE_ICON: Record<VizSource["kind"], LucideIcon> = {
  file: FileTextIcon,
  query: DatabaseIcon,
  sync: DatabaseIcon,
};

function SourceNode({ source }: { source: VizSource }) {
  const Icon = SOURCE_ICON[source.kind];
  const StatusIcon = source.ready ? CheckIcon : XIcon;
  return (
    <div className="inline-flex items-center gap-[5px] px-[10px] py-[4px] text-neutral-500 text-[11px] whitespace-nowrap">
      <Icon size={12} />
      <span className="overflow-hidden text-ellipsis">{source.label}</span>
      <StatusIcon
        size={12}
        className={source.ready ? "text-green-600" : "text-red-600"}
        strokeWidth={2.5}
      />
    </div>
  );
}

export function StepVisualize({
  description,
  viz,
}: {
  description?: string;
  viz: VizRow;
}) {
  const disabled = !viz.canRun || viz.status === "running";
  const btnStyle =
    viz.status === "done" ? doneBtn : viz.canRun ? activeBtn : notReadyBtn;

  // An unknown type is a profile error the verifier already flags; render the
  // fallback rather than crashing a step the user is looking at.
  const Viz = VIZ_REGISTRY[viz.spec.type] ?? VIZ_REGISTRY.table;

  return (
    <div className="border border-[#e5e2dc] bg-white px-[18px] py-[16px]">
      {description && (
        <p className="text-[13px] text-neutral-600 mb-[12px] whitespace-pre-line">
          {description}
        </p>
      )}

      {/* What it's drawing, and whether each piece is ready yet */}
      <div className="flex items-center gap-[8px] mb-[10px]">
        <BarChart3Icon size={16} className="text-neutral-400 shrink-0" />
        <div className="flex flex-wrap items-center gap-[2px] min-w-0">
          {viz.sources.length > 0 ? (
            viz.sources.map((s) => <SourceNode key={`${s.kind}:${s.label}`} source={s} />)
          ) : (
            <span className="px-[10px] py-[4px] text-[11px] text-neutral-400">
              no sources
            </span>
          )}
        </div>
      </div>

      <div className="flex items-center gap-[8px]">
        <Button
          type="button"
          onClick={viz.onRun}
          disabled={disabled}
          className={`${baseBtn} ${btnStyle}`}
        >
          <BarChart3Icon size={13} />
          {viz.status === "running"
            ? "Loading…"
            : viz.status === "done"
            ? "Refresh"
            : "Show Data"}
        </Button>

        {/* Indeterminate — a local DuckDB SELECT reports no progress. */}
        <div className="flex-1 h-[5px] bg-neutral-200 overflow-hidden">
          {viz.status === "running" ? (
            <div className="h-full w-1/3 bg-neutral-400 animate-pulse" />
          ) : (
            <div
              className={cn(
                "h-full transition-[width] duration-200",
                viz.status === "done"
                  ? "w-full bg-green-500"
                  : viz.status === "error"
                  ? "w-full bg-red-500"
                  : "w-0",
              )}
            />
          )}
        </div>

        {viz.status === "done" && viz.data && (
          <span className="text-[11px] text-neutral-500 whitespace-nowrap">
            {viz.data.rows.length} row{viz.data.rows.length === 1 ? "" : "s"}
          </span>
        )}
      </div>

      {viz.status === "done" && viz.data && (
        <div className="mt-[12px] border border-[#e5e2dc]">
          {viz.spec.title && (
            <div className="px-[10px] py-[7px] border-b border-[#e5e2dc] text-[12px] font-medium text-neutral-700">
              {viz.spec.title}
            </div>
          )}
          <Viz data={viz.data} config={viz.spec.config} title={viz.spec.title} />
        </div>
      )}

      {viz.status === "error" && viz.error && (
        <div className="mt-[12px] border border-red-200 bg-red-50 px-[12px] py-[9px] text-[12px] text-red-800">
          {viz.error}
        </div>
      )}
    </div>
  );
}
