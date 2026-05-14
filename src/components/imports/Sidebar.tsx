// Sidebar.tsx
//
// Fixed-width left rail: profile select on top, step checklist below.
// The vertical rule under the step list aligns with the horizontal center
// of the profile icon above it.

import { useState } from "react";
import {
  FileInputIcon,
  CheckIcon,
  RotateCcwIcon,
  ChevronsUpDownIcon,
} from "lucide-react";
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
import type { LoadedProfile, ProfileSummary } from "../../types";

type SidebarProps = {
  profiles: ProfileSummary[];
  // Holds the zip_path of the selected profile (unique across builtin + user),
  // not the id — ids can collide between sources.
  selectedProfile: string | null;
  onSelectProfile: (zipPath: string) => void;
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
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const selected = profiles.find((p) => p.zip_path === selectedProfile);
  const query = search.trim().toLowerCase();

  return (
    <aside className="w-[214px] shrink-0 border-r border-neutral-200 flex flex-col pt-[15px]">
      {/* Profile combobox row */}
      <div className="flex items-center gap-[7px] px-[10px] pb-[10px]">
        <FileInputIcon size={16} className="text-neutral-400 shrink-0" />
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger
            role="combobox"
            aria-expanded={open}
            className="h-[30px] w-[170px] flex items-center justify-between gap-1.5 rounded-lg border border-input bg-transparent px-2.5 text-[13px] outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 cursor-pointer hover:bg-neutral-50"
          >
            <span
              className={cn(
                "min-w-0 truncate",
                selected ? "text-neutral-900" : "text-neutral-400"
              )}
            >
              {selected?.name ?? "Select profile…"}
            </span>
            <ChevronsUpDownIcon className="size-4 shrink-0 opacity-50" />
          </PopoverTrigger>
          <PopoverContent className="w-[300px] p-0" align="start">
            <Command>
              <CommandInput
                placeholder="Search profiles"
                className="text-[13px]"
                value={search}
                onValueChange={setSearch}
              />
              <CommandList>
                <CommandEmpty>No profile found 😵‍💫</CommandEmpty>
                <CommandGroup>
                  {profiles.map((p) => {
                    const idMatches =
                      query.length > 0 && p.id.toLowerCase().includes(query);
                    return (
                      <CommandItem
                        key={p.zip_path}
                        value={`${p.name}::${p.zip_path}`}
                        keywords={[p.id, p.source]}
                        onSelect={() => {
                          onSelectProfile(p.zip_path);
                          setOpen(false);
                        }}
                        className="text-[13px]"
                      >
                        <span className="flex-1 min-w-0 flex items-center gap-1.5">
                          <span className="truncate">{p.name}</span>
                          {idMatches && (
                          <span className="text-neutral-400 truncate tracking-[-0.02em]">
                            {p.id}
                          </span>
                        )}
                          {p.source === "builtin" && (
                            <span className="text-[9px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
                              built-in
                            </span>
                          )}
                        </span>
                        <CheckIcon
                          size={14}
                          className={cn(
                            "shrink-0",
                            selectedProfile === p.zip_path
                              ? "opacity-100"
                              : "opacity-0"
                          )}
                        />
                      </CommandItem>
                    );
                  })}
                </CommandGroup>
              </CommandList>
            </Command>
          </PopoverContent>
        </Popover>
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
                <button
                  key={step.label}
                  type="button"
                  onClick={() => {
                    const el = document.getElementById(`step-${step.label}`);
                    if (!el) return;
                    el.scrollIntoView({ behavior: "smooth", block: "start" });
                    // Retrigger the shimmer if the same step is clicked again.
                    el.classList.remove("step-flash");
                    void el.offsetWidth;
                    el.classList.add("step-flash");
                  }}
                  className="flex items-center justify-between px-[6px] py-[5px] rounded-md hover:bg-neutral-50 text-left cursor-pointer bg-transparent border-0"
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
                </button>
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
