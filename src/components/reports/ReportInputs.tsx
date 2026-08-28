// ReportInputs.tsx
//
// Renders a report's `parameters` as form controls in the Inputs panel —
// deliberately separate from the visualizations. Each control writes its value
// back through onParamChange; the parent (App.tsx) holds the values and feeds
// them to run_report on Refresh. Supported types: date_range, date, select,
// text, number.

import type { Parameter } from "../../types";

type DateRange = { from?: string; to?: string };

type ReportInputsProps = {
  parameters: Parameter[];
  values: Record<string, unknown>;
  onChange: (id: string, value: unknown) => void;
};

const inputClass =
  "h-[30px] rounded-md border border-input bg-transparent px-2.5 text-[13px] outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50";

function ParamControl({
  param,
  value,
  onChange,
}: {
  param: Parameter;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  switch (param.type) {
    case "date_range": {
      const v = (value ?? {}) as DateRange;
      return (
        <div className="flex items-center gap-[6px]">
          <input
            type="date"
            className={inputClass}
            value={v.from ?? ""}
            onChange={(e) => onChange({ ...v, from: e.target.value })}
          />
          <span className="text-[13px] text-neutral-400">→</span>
          <input
            type="date"
            className={inputClass}
            value={v.to ?? ""}
            onChange={(e) => onChange({ ...v, to: e.target.value })}
          />
        </div>
      );
    }
    case "date":
      return (
        <input
          type="date"
          className={inputClass}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "select":
      return (
        <select
          className={inputClass + " cursor-pointer"}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        >
          <option value="">Select…</option>
          {(param.options ?? []).map((opt) => (
            <option key={opt} value={opt}>
              {opt}
            </option>
          ))}
        </select>
      );
    case "number":
      return (
        <input
          type="number"
          className={inputClass + " w-[160px]"}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    default: // text
      return (
        <input
          type="text"
          className={inputClass + " w-[220px]"}
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
        />
      );
  }
}

export function ReportInputs({ parameters, values, onChange }: ReportInputsProps) {
  if (parameters.length === 0) {
    return (
      <p className="text-[13px] text-neutral-400">This report takes no inputs.</p>
    );
  }
  return (
    <div className="flex flex-wrap items-end gap-x-[18px] gap-y-[10px]">
      {parameters.map((p) => (
        <label key={p.id} className="flex flex-col gap-[5px]">
          <span className="text-[12px] font-medium text-neutral-600">
            {p.label}
            {p.required && <span className="text-red-400"> *</span>}
          </span>
          <ParamControl
            param={p}
            value={values[p.id]}
            onChange={(v) => onChange(p.id, v)}
          />
        </label>
      ))}
    </div>
  );
}
