// ProfileSidebar.tsx
//
// Left column of the Import-profiles tab: search box, scrollable profile
// list, and the "New profile" button pinned to the bottom.
// Selection keys on zip_path (not id) — a built-in and a user profile can
// share an id; only zip_path is unique.

import { useMemo } from "react";
import { FilePlusIcon, SearchIcon, XIcon } from "lucide-react";
import { cn } from "../../../lib/utils";
import type { ProfileSummary } from "../../../types";

type Props = {
  profiles: ProfileSummary[];
  selectedZipPath: string | null;
  query: string;
  onQueryChange: (q: string) => void;
  onSelect: (summary: ProfileSummary) => void;
  onNewProfile: () => void;
  busy: boolean;
};

export function ProfileSidebar({
  profiles,
  selectedZipPath,
  query,
  onQueryChange,
  onSelect,
  onNewProfile,
  busy,
}: Props) {
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return profiles;
    return profiles.filter(
      (p) => p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q),
    );
  }, [profiles, query]);

  return (
    <aside className="w-[220px] shrink-0 border-r border-neutral-200 flex flex-col bg-neutral-50/60">
      <div className="px-[8px] pt-[8px] pb-[4px]">
        <div className="relative">
          <SearchIcon
            size={12}
            className="absolute left-[8px] top-1/2 -translate-y-1/2 text-neutral-400 pointer-events-none"
          />
          <input
            type="text"
            value={query}
            onChange={(e) => onQueryChange(e.target.value)}
            placeholder="Search profiles"
            className="w-full text-[12px] pl-[24px] pr-[24px] py-[5px] rounded-[6px] border border-neutral-200 bg-white text-neutral-800 placeholder:text-neutral-400 outline-none focus:border-neutral-400"
          />
          {query && (
            <button
              type="button"
              onClick={() => onQueryChange("")}
              aria-label="Clear search"
              className="absolute right-[4px] top-1/2 -translate-y-1/2 w-[18px] h-[18px] rounded-[4px] inline-flex items-center justify-center text-neutral-400 hover:text-neutral-700 hover:bg-black/5 border-0 bg-transparent cursor-pointer"
            >
              <XIcon size={12} />
            </button>
          )}
        </div>
      </div>
      <ul className="flex-1 overflow-auto p-[6px] pt-[6px] flex flex-col gap-[5px] list-none m-0">
        {filtered.length === 0 && (
          <li className="px-[10px] py-[10px] text-[12px] text-neutral-400 text-center">
            No matches
          </li>
        )}
        {filtered.map((p) => {
          const isSelected = p.zip_path === selectedZipPath;
          return (
            <li key={p.zip_path}>
              <button
                type="button"
                onClick={() => onSelect(p)}
                disabled={busy}
                className={cn(
                  "w-full text-left px-[10px] py-[6px] rounded-[6px] border-0 transition-colors cursor-pointer disabled:cursor-wait",
                  isSelected
                    ? "bg-neutral-200/70"
                    : "bg-transparent hover:bg-black/5",
                )}
              >
                <div className="flex items-center justify-between gap-[6px]">
                  <span
                    className={cn(
                      "text-[13px] text-neutral-900 truncate",
                      isSelected ? "font-medium" : "font-normal",
                    )}
                  >
                    {p.name}
                  </span>
                  {p.source === "builtin" && (
                    <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
                      built-in
                    </span>
                  )}
                </div>
                <div className="text-[11px] text-neutral-500 mt-[1px]">
                  {p.id} v{p.version}
                </div>
              </button>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        onClick={onNewProfile}
        disabled={busy}
        className="m-[8px] px-[10px] py-[7px] rounded-[7px] border border-dashed border-neutral-300 text-[12px] text-neutral-600 bg-transparent hover:bg-black/5 inline-flex items-center justify-center gap-[6px] cursor-pointer disabled:cursor-wait disabled:opacity-60"
      >
        <FilePlusIcon size={13} />
        New profile
      </button>
    </aside>
  );
}
