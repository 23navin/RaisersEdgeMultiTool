// ReportsPage.tsx
//
// The Reports tab. Top bar: a search control (popover) to pick a report on the
// left, the selected report's name on the right. Body: an Inputs panel (the
// report's parameters + a Refresh button) kept separate from the Visualizations
// area below. Visualizations start blank and are filled by Refresh, which runs
// the backend pipeline (run_report) via App.tsx. Changing an input marks the
// visualizations stale until the next Refresh.
//
// State + all invoke() calls live in App.tsx; this component is props-driven.

import { useState } from "react";
import { SearchIcon, RefreshCwIcon } from "lucide-react";
import { Panel } from "../shared/Panel";
import { Button } from "../ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "../ui/popover";
import {
  Command,
  CommandInput,
  CommandList,
  CommandEmpty,
  CommandGroup,
  CommandItem,
} from "../ui/command";
import { cn } from "@/lib/utils";
import type { LoadedProfile, ProfileSummary, ReportRunResult } from "../../types";
import { ReportInputs } from "./ReportInputs";
import { VIZ_REGISTRY } from "./viz";

export type ReportStatus = "idle" | "running" | "done" | "error";
export type ActionState = {
  status: "idle" | "running" | "done" | "error";
  message?: string;
};

type ReportsPageProps = {
  reports: ProfileSummary[];
  selectedReport: string | null;
  onSelectReport: (zipPath: string) => void;
  loadedReport: LoadedProfile | null;
  paramValues: Record<string, unknown>;
  onParamChange: (id: string, value: unknown) => void;
  run: ReportRunResult | null;
  status: ReportStatus;
  error?: string | null;
  stale: boolean;
  onRefresh: () => void;
  actionStates: Record<string, ActionState>;
  onRunAction: (actionId: string) => void;
};

export function ReportsPage({
  reports,
  selectedReport,
  onSelectReport,
  loadedReport,
  paramValues,
  onParamChange,
  run,
  status,
  error,
  stale,
  onRefresh,
  actionStates,
  onRunAction,
}: ReportsPageProps) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [search, setSearch] = useState("");

  const selected = reports.find((r) => r.zip_path === selectedReport);
  const structure = loadedReport?.structure;

  return (
    <Panel className="flex flex-col h-full">
      {/* Top bar — search (left) + selected report (right) */}
      <div className="h-[44px] shrink-0 flex items-center justify-between px-[12px] border-b border-neutral-200">
        <Popover open={searchOpen} onOpenChange={setSearchOpen}>
          <PopoverTrigger
            aria-label="Search reports"
            aria-expanded={searchOpen}
            className="w-[30px] h-[30px] rounded-md inline-flex items-center justify-center text-neutral-500 outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 cursor-pointer hover:bg-neutral-50 aria-expanded:bg-neutral-100 aria-expanded:text-neutral-900"
          >
            <SearchIcon className="size-4" />
          </PopoverTrigger>
          <PopoverContent className="w-[300px] p-0" align="start">
            <Command>
              <CommandInput
                placeholder="Search reports"
                className="text-[13px]"
                value={search}
                onValueChange={setSearch}
              />
              <CommandList>
                <CommandEmpty>No reports found 🫥</CommandEmpty>
                <CommandGroup>
                  {reports.map((r) => (
                    <CommandItem
                      key={r.zip_path}
                      value={`${r.name}::${r.zip_path}`}
                      keywords={[r.id, r.source]}
                      onSelect={() => {
                        onSelectReport(r.zip_path);
                        setSearchOpen(false);
                      }}
                      className="text-[13px]"
                    >
                      <span className="truncate">{r.name}</span>
                      {r.source === "builtin" && (
                        <span className="text-[9px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
                          built-in
                        </span>
                      )}
                    </CommandItem>
                  ))}
                </CommandGroup>
              </CommandList>
            </Command>
          </PopoverContent>
        </Popover>

        <span
          className={cn(
            "text-[13px] truncate min-w-0 pl-[10px]",
            selected ? "text-neutral-900" : "text-neutral-400",
          )}
        >
          {selected?.name ?? "No report selected"}
        </span>
      </div>

      {/* Body */}
      {!structure ? (
        <main className="flex-1 grid place-items-center text-[13px] text-neutral-400">
          Select a report to begin.
        </main>
      ) : (
        <main className="flex-1 overflow-y-auto">
          {/* Inputs panel — separate from the visualizations */}
          <section className="px-[22px] py-[16px] border-b border-neutral-200 bg-neutral-50/50">
            <div className="flex items-end justify-between gap-[16px] flex-wrap">
              <ReportInputs
                parameters={structure.parameters}
                values={paramValues}
                onChange={onParamChange}
              />
              <div className="flex flex-col items-end gap-[4px]">
                <Button onClick={onRefresh} disabled={status === "running"}>
                  <RefreshCwIcon
                    className={status === "running" ? "animate-spin" : undefined}
                  />
                  {status === "running" ? "Refreshing…" : "Refresh"}
                </Button>
                {run && status !== "running" && (
                  <span className="text-[11px] text-neutral-400">
                    Updated {new Date(run.generated_at).toLocaleTimeString()}
                  </span>
                )}
              </div>
            </div>
            {status === "error" && error && (
              <p className="mt-[10px] text-[12px] text-red-500">{error}</p>
            )}
            {stale && status !== "running" && run && (
              <p className="mt-[10px] text-[12px] text-amber-600">
                Inputs changed — Refresh to update the data.
              </p>
            )}
          </section>

          {/* Visualizations area */}
          <section className="px-[22px] py-[18px] flex flex-col gap-[18px]">
            {structure.visualizations.length === 0 && (
              <p className="text-[13px] text-neutral-400">
                This report defines no visualizations.
              </p>
            )}
            {structure.visualizations.map((viz) => {
              const Comp = VIZ_REGISTRY[viz.type];
              const rs = run?.data[viz.data];
              return (
                <div
                  key={viz.id}
                  className="rounded-lg border border-neutral-200 overflow-hidden"
                >
                  <div className="px-[14px] h-[36px] flex items-center border-b border-neutral-200 bg-neutral-50">
                    <h3 className="text-[13px] font-medium text-neutral-800">
                      {viz.title ?? viz.id}
                    </h3>
                  </div>
                  <div className={cn(stale && "opacity-50")}>
                    {!run ? (
                      <div className="px-[14px] py-[18px] text-[13px] text-neutral-400">
                        {status === "running"
                          ? "Loading…"
                          : "Press Refresh to load data."}
                      </div>
                    ) : !Comp ? (
                      <div className="px-[14px] py-[12px] text-[13px] text-red-500">
                        Unknown visualization type “{viz.type}”.
                      </div>
                    ) : !rs ? (
                      <div className="px-[14px] py-[12px] text-[13px] text-neutral-400">
                        No data produced for “{viz.data}”.
                      </div>
                    ) : (
                      <Comp data={rs} config={viz.config} title={viz.title} />
                    )}
                  </div>
                </div>
              );
            })}

            {/* Actions */}
            {structure.actions.length > 0 && (
              <div className="flex flex-col gap-[8px] pt-[2px]">
                <div className="flex flex-wrap items-center gap-[10px]">
                  {structure.actions.map((action) => {
                    const st = actionStates[action.id];
                    return (
                      <Button
                        key={action.id}
                        variant="outline"
                        disabled={!run || st?.status === "running"}
                        onClick={() => onRunAction(action.id)}
                      >
                        {st?.status === "running"
                          ? "Working…"
                          : action.label}
                      </Button>
                    );
                  })}
                </div>
                {structure.actions.map((action) => {
                  const st = actionStates[action.id];
                  if (!st?.message) return null;
                  return (
                    <p
                      key={action.id}
                      className={cn(
                        "text-[12px]",
                        st.status === "error"
                          ? "text-red-500"
                          : "text-green-600",
                      )}
                    >
                      {st.message}
                    </p>
                  );
                })}
              </div>
            )}
          </section>
        </main>
      )}
    </Panel>
  );
}
