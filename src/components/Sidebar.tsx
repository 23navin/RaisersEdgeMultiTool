// Sidebar.tsx
//
// Fixed-width left rail: profile select on top, step checklist below.
// The vertical rule under the step list aligns with the horizontal center
// of the profile icon above it.

import { FileInputIcon, CheckIcon, RotateCcwIcon } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./ui/select";
import type { LoadedProfile, ProfileSummary } from "../types";

type SidebarProps = {
  profiles: ProfileSummary[];
  selectedProfile: string | null;
  onSelectProfile: (id: string) => void;
  loadedProfile: LoadedProfile | null;
  stepsDone: Record<string, boolean>;
  onReset: () => void;
};

// Pulls a display label out of the ## heading in the step's markdown,
// falling back to the raw YAML label.
function stepDisplayName(label: string, instructions: Record<string, string>) {
  const md = instructions[label];
  if (!md) return label;
  const m = md.match(/^##\s+(.+)$/m);
  return m ? m[1].trim() : label;
}

export function Sidebar({
  profiles,
  selectedProfile,
  onSelectProfile,
  loadedProfile,
  stepsDone,
  onReset,
}: SidebarProps) {
  return (
    <aside className="w-[214px] shrink-0 border-r border-neutral-200 flex flex-col pt-[10px]">
      {/* Profile select row */}
      <div className="flex items-center gap-[7px] px-[10px] pb-[10px]">
        <FileInputIcon size={16} className="text-neutral-400 shrink-0" />
        <Select
          value={selectedProfile ?? undefined}
          onValueChange={onSelectProfile}
        >
          <SelectTrigger className="h-[30px] w-[170px] text-[13px] *:data-[slot=select-value]:block *:data-[slot=select-value]:min-w-0 *:data-[slot=select-value]:truncate">
            <SelectValue placeholder="Select profile…" />
          </SelectTrigger>
          <SelectContent>
            {profiles.map((p) => (
              <SelectItem key={p.id} value={p.id}>
                {p.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {/* Step list — vertical rule aligns with the icon's horizontal center.
          flex-1 pushes the reset button to the bottom of the sidebar. */}
      <div className="flex-1 overflow-y-auto">
        {loadedProfile && (
          <nav className="ml-[17px] pl-[14px] mr-[10px] border-l-[1.5px] border-neutral-200 flex flex-col gap-[1px]">
            {loadedProfile.structure.steps.map((step) => {
              const done = stepsDone[step.label] ?? false;
              const name = stepDisplayName(step.label, loadedProfile.instructions);
              return (
                <div
                  key={step.label}
                  className="flex items-center justify-between px-[6px] py-[5px] rounded-md hover:bg-neutral-50"
                >
                  <span
                    className={
                      "text-[13px] " +
                      (done ? "text-neutral-900" : "text-neutral-500")
                    }
                  >
                    {name}
                  </span>
                  {done && <CheckIcon size={13} className="text-green-500" />}
                </div>
              );
            })}
          </nav>
        )}
      </div>

      {/* Footer — reset button */}
      <div className="border-t border-neutral-200 px-[10px] py-[8px]">
        <button
          type="button"
          onClick={onReset}
          disabled={!loadedProfile}
          className="w-full inline-flex items-center gap-[7px] px-[8px] py-[6px] rounded-md text-[13px] text-neutral-500 hover:bg-neutral-50 hover:text-neutral-900 border-0 bg-transparent disabled:opacity-40 disabled:hover:bg-transparent disabled:hover:text-neutral-500"
        >
          <RotateCcwIcon size={13} />
          Reset profile
        </button>
      </div>
    </aside>
  );
}
