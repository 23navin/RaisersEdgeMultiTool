// viz/index.tsx
//
// The report visualization registry. Maps a visualization's `type` to its
// component. ReportsPage looks up VIZ_REGISTRY[viz.type] and renders it with the
// bound ResultSet + config — mirroring how the imports MainPanel dispatches step
// types to step components.
//
// Only `table` is implemented. Bar/Line/Pie/Kpi are registered so a profile
// declaring them still renders (as a placeholder card naming the bound data)
// instead of blowing up; the intended implementation is Recharts. Profile
// authors are warned about this in REPORT_PROFILES.md and PROFILE_AUTHORING.md —
// keep those in sync when the charts land.

import type { VizComponent, VizProps } from "./types";
import { TableViz } from "./TableViz";

// Temporary placeholder shared by the chart stubs. Renders the viz title and a
// hint of the bound data until the Recharts components land.
function Placeholder({ kind, title, data }: VizProps & { kind: string }) {
  return (
    <div className="rounded-lg border border-dashed border-neutral-300 px-[14px] py-[12px] text-[13px] text-neutral-500">
      <div className="font-medium text-neutral-700">{title ?? kind}</div>
      <div className="text-neutral-400">
        {kind} · {data.rows.length} rows × {data.columns.length} cols
      </div>
    </div>
  );
}

// Intended: Recharts.
export const BarViz: VizComponent = (props) => (
  <Placeholder kind="bar" {...props} />
);
export const LineViz: VizComponent = (props) => (
  <Placeholder kind="line" {...props} />
);
export const PieViz: VizComponent = (props) => (
  <Placeholder kind="pie" {...props} />
);
export const KpiViz: VizComponent = (props) => (
  <Placeholder kind="kpi" {...props} />
);

export { TableViz };

export const VIZ_REGISTRY: Record<string, VizComponent> = {
  table: TableViz,
  bar: BarViz,
  line: LineViz,
  pie: PieViz,
  kpi: KpiViz,
};

export type { VizComponent, VizProps } from "./types";
