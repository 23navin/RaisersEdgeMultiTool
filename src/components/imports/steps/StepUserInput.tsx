// StepUserInput.tsx
//
// Renders steps with type=user_input.
// The step asks the operator for values the uploaded files don't carry — a
// start and end date per semester code, a batch number, a gift type to apply —
// and publishes them to later SQL as {{form:Label}}.
//
// Where the rows come from decides the shape on screen: a step with `rows_sql`
// draws one labelled row of controls per row that SQL returned (so the file
// itself decides how many semesters there are); a step without it draws a
// single unlabelled row. Either way the controls come from `fields`.
//
// Like StepVisualize, the step runs itself: it publishes as soon as its sources
// are ready, and again — debounced — after each edit, so the file downstream
// SQL reads is always what the boxes currently say. Nothing here is a gate; the
// step counts as done once every required box is filled.

import { useEffect, useRef } from "react";
import {
  PencilLineIcon,
  FileTextIcon,
  DatabaseIcon,
  CheckIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react";
import type { UserInputField, UserInputResult } from "../../../types";

export type UserInputStatus = "idle" | "running" | "done" | "error";

// One upstream source feeding the step's rows_sql. Same vocabulary the
// visualization step uses — a file the user uploaded vs. a named result an
// earlier step published.
export type FormSource = {
  label: string;
  ready: boolean;
  kind: "file" | "query" | "sync";
};

export type FormCard = {
  formOutput: string;
  fields: UserInputField[];
  // True when the step draws one row per rows_sql row (rather than a single
  // unlabelled row) — decides whether the key column is shown.
  keyed: boolean;
  sources: FormSource[];
  canRun: boolean;
  status: UserInputStatus;
  result?: UserInputResult;
  // What the boxes hold right now, row key → field id → value. Held in App.tsx
  // so it survives the step re-running and any upstream invalidation.
  values: Record<string, Record<string, string>>;
  error?: string;
  onRun: () => void;
  onChange: (key: string, fieldId: string, value: string) => void;
};

// How long to sit on an edit before republishing. Long enough that typing a
// date doesn't fire a call per keystroke, short enough that moving to the next
// box has already saved the last one.
const EDIT_DEBOUNCE_MS = 450;

const SOURCE_ICON: Record<FormSource["kind"], LucideIcon> = {
  file: FileTextIcon,
  query: DatabaseIcon,
  sync: DatabaseIcon,
};

const inputBase =
  "h-[28px] border border-[#e5e2dc] bg-white px-[8px] text-[12px] text-neutral-800 " +
  "focus:outline-none focus:border-neutral-400 disabled:bg-neutral-50 disabled:text-neutral-400";

function SourceNode({ source }: { source: FormSource }) {
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

// One control. The field's declared type picks the input; `select` gets the
// declared options and everything else is a plain box of the matching type.
function FieldControl({
  field,
  value,
  missing,
  onChange,
}: {
  field: UserInputField;
  value: string;
  missing: boolean;
  onChange: (value: string) => void;
}) {
  const cls = `${inputBase} ${missing ? "border-amber-400" : ""}`;
  if (field.type === "select") {
    return (
      <select
        className={`${cls} cursor-pointer`}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">—</option>
        {(field.options ?? []).map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
    );
  }
  return (
    <input
      type={field.type === "date" ? "date" : field.type === "number" ? "number" : "text"}
      className={cls}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

export function StepUserInput({
  description,
  form,
}: {
  description?: string;
  form: FormCard;
}) {
  // Self-publishing, on two different edges.
  //
  // The first is readiness: `idle` means "nothing published" — a fresh step, or
  // one App.tsx reset because something upstream changed — so an idle step with
  // every source ready should discover its rows and publish immediately. The
  // ref collapses that edge to one run (StrictMode fires effects twice).
  const fired = useRef(false);
  useEffect(() => {
    if (!form.canRun || form.status !== "idle") {
      fired.current = false;
      return;
    }
    if (fired.current) return;
    fired.current = true;
    form.onRun();
    // onRun is rebuilt on every render of App; the readiness edge is the
    // trigger, so it is deliberately not a dependency.
  }, [form.canRun, form.status]); // eslint-disable-line react-hooks/exhaustive-deps

  // The second is editing: republish what the boxes now say, once the user
  // pauses. Keyed on the serialized values so an unrelated re-render doesn't
  // queue a call, and cleared on each change so a burst of typing costs one.
  // An errored step listens too — otherwise a failed publish would be a dead
  // end, since the readiness edge above only fires from `idle`.
  const serialized = JSON.stringify(form.values);
  const lastPublished = useRef<string | null>(null);
  useEffect(() => {
    if (!form.canRun || (form.status !== "done" && form.status !== "error")) return;
    if (lastPublished.current === null) {
      // First result after a run — adopt it as the baseline rather than
      // republishing what the backend just told us.
      lastPublished.current = serialized;
      return;
    }
    if (lastPublished.current === serialized) return;
    const t = setTimeout(() => {
      lastPublished.current = serialized;
      form.onRun();
    }, EDIT_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [serialized, form.canRun, form.status]); // eslint-disable-line react-hooks/exhaustive-deps

  // A run that starts from scratch (readiness edge) drops the baseline so the
  // next result re-adopts it.
  useEffect(() => {
    if (form.status === "idle") lastPublished.current = null;
  }, [form.status]);

  const rows = form.result?.rows ?? [];
  const filledLabel = form.result
    ? form.result.complete
      ? "all values set"
      : `${rows.filter((r) => r.missing.length > 0).length} still blank`
    : null;

  return (
    <div className="border border-[#e5e2dc] bg-white px-[18px] py-[16px]">
      {description && (
        <p className="text-[13px] text-neutral-600 mb-[12px] whitespace-pre-line">
          {description}
        </p>
      )}

      {/* What the row list is built from, and how the last publish went. */}
      <div className="flex items-center gap-[8px]">
        <PencilLineIcon size={16} className="text-neutral-400 shrink-0" />
        <div className="flex flex-wrap items-center gap-[2px] min-w-0 flex-1">
          {form.sources.length > 0 ? (
            form.sources.map((s) => <SourceNode key={`${s.kind}:${s.label}`} source={s} />)
          ) : (
            <span className="px-[10px] py-[4px] text-[11px] text-neutral-400">
              no sources
            </span>
          )}
        </div>
        {form.status === "running" && (
          <span className="text-[11px] text-neutral-400 whitespace-nowrap">Saving…</span>
        )}
        {form.status === "done" && filledLabel && (
          <span
            className={`text-[11px] whitespace-nowrap ${
              form.result?.complete ? "text-neutral-500" : "text-amber-700"
            }`}
          >
            {filledLabel}
          </span>
        )}
        {form.status === "idle" && !form.canRun && (
          <span className="text-[11px] text-neutral-400 whitespace-nowrap">
            waiting on sources
          </span>
        )}
      </div>

      {/* The form itself. One row of controls per row the step publishes.
          Drawn from the last result rather than gated on status: the step
          republishes on every edit, and a form that blinked out mid-keystroke
          — or vanished when a publish failed — would be unusable. */}
      {rows.length > 0 && (
        <div className="mt-[12px] border border-[#e5e2dc]">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="border-b border-[#e5e2dc] bg-[#faf9f7]">
                {form.keyed && (
                  <th className="text-left font-medium text-neutral-600 px-[10px] py-[7px]">
                    {rows[0].columns[0] ?? "Value"}
                  </th>
                )}
                {form.fields.map((f) => (
                  <th
                    key={f.id}
                    className="text-left font-medium text-neutral-600 px-[10px] py-[7px]"
                  >
                    {f.label}
                    {f.required && <span className="text-neutral-400"> *</span>}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.key} className="border-b border-[#f0eee9] last:border-b-0">
                  {form.keyed && (
                    <td className="px-[10px] py-[6px] text-neutral-800 whitespace-nowrap">
                      {row.display.join(" · ")}
                    </td>
                  )}
                  {form.fields.map((f) => (
                    <td key={f.id} className="px-[10px] py-[6px]">
                      <FieldControl
                        field={f}
                        value={form.values[row.key]?.[f.id] ?? row.values[f.id] ?? ""}
                        missing={row.missing.includes(f.id)}
                        onChange={(v) => form.onChange(row.key, f.id, v)}
                      />
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* A form with no rows is a legitimate outcome — the file carried nothing
          to ask about — and downstream SQL still gets an empty [] to join to. */}
      {form.status === "done" && rows.length === 0 && (
        <div className="mt-[12px] border border-[#e5e2dc] px-[12px] py-[9px] text-[12px] text-neutral-500">
          Nothing to fill in — the source rows are empty.
        </div>
      )}

      {form.status === "error" && form.error && (
        <div className="mt-[12px] border border-red-200 bg-red-50 px-[12px] py-[9px] text-[12px] text-red-800">
          {form.error}
        </div>
      )}
    </div>
  );
}
