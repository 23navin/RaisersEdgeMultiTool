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

import { useEffect, useRef, useState } from "react";
import {
  PencilLineIcon,
  FileTextIcon,
  DatabaseIcon,
  CheckIcon,
  ChevronsUpDownIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react";
import { Popover, PopoverTrigger, PopoverContent } from "../../ui/popover";
import {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
} from "../../ui/command";
import type { FieldOption, UserInputField, UserInputResult } from "../../../types";

export type UserInputStatus = "idle" | "running" | "done" | "error";

// One upstream source feeding the step's rows_sql. Same vocabulary the
// visualization step uses — a file the user uploaded vs. a named result an
// earlier step published.
export type FormSource = {
  label: string;
  ready: boolean;
  kind: "file" | "query" | "sync" | "form";
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
  form: PencilLineIcon,
};

// Below this many choices a plain <select> is the better control: no popover,
// no search box, keyboard-navigable out of the box. Above it, searching beats
// scrolling — a fund list runs to hundreds of entries.
const SEARCHABLE_FROM = 8;

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

// A select over a long list: a searchable popover rather than a dropdown you
// scroll. Matching runs over both the stored value and the label, so a fund is
// findable by its id or by its description.
function SearchableSelect({
  options,
  value,
  invalid,
  onChange,
}: {
  options: FieldOption[];
  value: string;
  invalid: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const chosen = options.find((o) => o.value === value);
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger
        role="combobox"
        aria-expanded={open}
        className={`${inputBase} ${invalid ? "border-amber-400" : ""} flex w-full min-w-[190px] items-center justify-between gap-[6px] cursor-pointer hover:bg-neutral-50`}
      >
        <span className={chosen ? "truncate" : "truncate text-neutral-400"}>
          {chosen ? chosen.label : value ? `${value} (not in list)` : "—"}
        </span>
        <ChevronsUpDownIcon size={12} className="shrink-0 text-neutral-400" />
      </PopoverTrigger>
      <PopoverContent className="w-[320px] p-0" align="start">
        <Command>
          <CommandInput placeholder="Search" className="text-[12px]" />
          <CommandList>
            <CommandEmpty>No match</CommandEmpty>
            <CommandGroup>
              {/* Clearing is a legitimate answer — a row the operator decides
                  not to map stays unmapped rather than forcing a wrong pick. */}
              <CommandItem
                value="__clear__"
                onSelect={() => {
                  onChange("");
                  setOpen(false);
                }}
                className="text-[12px] text-neutral-500"
              >
                — none —
              </CommandItem>
              {options.map((o) => (
                <CommandItem
                  key={o.value}
                  value={o.label}
                  keywords={[o.value]}
                  onSelect={() => {
                    onChange(o.value);
                    setOpen(false);
                  }}
                  className="text-[12px]"
                >
                  <span className="truncate">{o.label}</span>
                  {o.value === value && (
                    <CheckIcon size={12} className="ml-auto text-green-600" />
                  )}
                </CommandItem>
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

// One control. The field's declared type picks the input; a `select` renders
// from the options the backend resolved — a fixed YAML list and an options_sql
// result arrive in the same shape, so this doesn't care which it was.
function FieldControl({
  field,
  options,
  value,
  missing,
  stale,
  onChange,
}: {
  field: UserInputField;
  options: FieldOption[];
  value: string;
  missing: boolean;
  stale: boolean;
  onChange: (value: string) => void;
}) {
  const flagged = missing || stale;
  const cls = `${inputBase} ${flagged ? "border-amber-400" : ""}`;
  if (field.type === "select") {
    if (options.length >= SEARCHABLE_FROM) {
      return (
        <SearchableSelect
          options={options}
          value={value}
          invalid={flagged}
          onChange={onChange}
        />
      );
    }
    return (
      <select
        className={`${cls} cursor-pointer`}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value="">—</option>
        {/* A held value the list no longer offers still needs somewhere to
            show, or the box would silently read as a different choice. */}
        {value && !options.some((o) => o.value === value) && (
          <option value={value}>{value} (not in list)</option>
        )}
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
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
  const blank = rows.filter((r) => r.missing.length > 0).length;
  const stale = rows.filter((r) => r.stale.length > 0).length;
  const filledLabel = form.result
    ? form.result.complete
      ? "all values set"
      : stale > 0 && blank === 0
        ? `${stale} no longer in the list`
        : `${blank} still blank`
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
                {/* Every column rows_sql selected gets its own header, under
                    the name the SQL gave it — the row's identity is often more
                    than one value (a project id AND the title it arrived
                    under), and reading them concatenated is no use. */}
                {form.keyed &&
                  rows[0].columns.map((c) => (
                    <th
                      key={c}
                      className="text-left font-medium text-neutral-600 px-[10px] py-[7px] whitespace-nowrap"
                    >
                      {c}
                    </th>
                  ))}
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
                  {form.keyed &&
                    row.columns.map((c, i) => (
                      <td
                        key={c}
                        className="px-[10px] py-[6px] text-neutral-800 align-middle"
                      >
                        {row.display[i] ?? ""}
                      </td>
                    ))}
                  {form.fields.map((f) => (
                    <td key={f.id} className="px-[10px] py-[6px] align-middle">
                      <FieldControl
                        field={f}
                        options={form.result?.options?.[f.id] ?? []}
                        value={form.values[row.key]?.[f.id] ?? row.values[f.id] ?? ""}
                        missing={row.missing.includes(f.id)}
                        stale={row.stale.includes(f.id)}
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
