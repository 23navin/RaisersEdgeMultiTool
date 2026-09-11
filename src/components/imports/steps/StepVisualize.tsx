// StepVisualize.tsx
//
// Renders steps with type=visualization.
// The step reads what earlier steps already produced — uploaded files, an
// re_query result, a code_table_sync outcome — and draws it on screen. Nothing
// is written and nothing is sent to RE, so there is no output file, no download
// and no live/mock badge: the data is already local by the time this runs.
//
// Because it only re-reads local data, the step is self-refreshing: it runs
// itself as soon as every source it declares is ready, and again after an
// upstream change invalidates the drawn result. There is nothing to press and
// no progress to report, so the button is a manual re-read, not a gate.
//
// The drawing itself is delegated to the report VIZ_REGISTRY, so a table here
// and a table on the Reports tab are the same component.

import { useEffect, useRef } from "react";
import {
  BarChart3Icon,
  FileTextIcon,
  DatabaseIcon,
  CheckIcon,
  XIcon,
  RefreshCwIcon,
  PencilLineIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "../../ui/button";
import { VIZ_REGISTRY } from "../../reports/viz";
import type { ResultSet, StepVisualization } from "../../../types";

export type VizStatus = "idle" | "running" | "done" | "error";

// One upstream source feeding the step. `kind` picks the icon: a file the user
// uploaded vs. a named result an earlier step published.
export type VizSource = {
  label: string;
  ready: boolean;
  kind: "file" | "query" | "sync" | "form";
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

// Secondary control: the step already refreshes itself, so this is only for a
// re-read when the underlying file changed outside the app.
const refreshBtn =
  "rounded-none h-[26px] w-[26px] p-0 shrink-0 border-0 shadow-none transform-gpu " +
  "bg-transparent hover:bg-[#efece7] text-neutral-500 hover:text-neutral-800 " +
  "disabled:opacity-40 disabled:hover:bg-transparent cursor-pointer disabled:cursor-not-allowed";

const SOURCE_ICON: Record<VizSource["kind"], LucideIcon> = {
  file: FileTextIcon,
  query: DatabaseIcon,
  sync: DatabaseIcon,
  form: PencilLineIcon,
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
  // Self-refresh. `idle` means "no result on hand" — the state a fresh step
  // starts in and the state App.tsx puts it back into whenever something
  // upstream is re-run or swapped — so an idle step whose sources are all ready
  // is exactly a step that should draw itself. The ref collapses each readiness
  // edge to a single run: StrictMode fires effects twice, and an unrelated
  // re-render must not queue a second identical query.
  const fired = useRef(false);
  useEffect(() => {
    if (!viz.canRun || viz.status !== "idle") {
      fired.current = false;
      return;
    }
    if (fired.current) return;
    fired.current = true;
    viz.onRun();
    // onRun is rebuilt on every render of App; the readiness edge is the
    // trigger, so it is deliberately not a dependency.
  }, [viz.canRun, viz.status]); // eslint-disable-line react-hooks/exhaustive-deps

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

      {/* What it's drawing, whether each piece is ready yet, and how the last
          run went — the step's whole status line now that it runs itself. */}
      <div className="flex items-center gap-[8px]">
        <BarChart3Icon size={16} className="text-neutral-400 shrink-0" />
        <div className="flex flex-wrap items-center gap-[2px] min-w-0 flex-1">
          {viz.sources.length > 0 ? (
            viz.sources.map((s) => <SourceNode key={`${s.kind}:${s.label}`} source={s} />)
          ) : (
            <span className="px-[10px] py-[4px] text-[11px] text-neutral-400">
              no sources
            </span>
          )}
        </div>

        {viz.status === "running" && (
          <span className="text-[11px] text-neutral-400 whitespace-nowrap">
            Loading…
          </span>
        )}
        {viz.status === "done" && viz.data && (
          <span className="text-[11px] text-neutral-500 whitespace-nowrap">
            {viz.data.rows.length} row{viz.data.rows.length === 1 ? "" : "s"}
          </span>
        )}
        {viz.status === "idle" && !viz.canRun && (
          <span className="text-[11px] text-neutral-400 whitespace-nowrap">
            waiting on sources
          </span>
        )}

        <Button
          type="button"
          onClick={viz.onRun}
          disabled={!viz.canRun || viz.status === "running"}
          title="Refresh"
          aria-label="Refresh"
          className={refreshBtn}
        >
          <RefreshCwIcon
            size={13}
            className={viz.status === "running" ? "animate-spin" : undefined}
          />
        </Button>
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
