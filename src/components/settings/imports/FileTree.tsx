// FileTree.tsx
//
// Left-side file navigator inside the profile editor. Folder structure is
// hidden from the user — files are bucketed into three fixed groups:
//   1. profile (structure.yaml + instructions.md)
//   2. transform SQL
//   3. notice SQL
// Notice SQL is identified by parsing `notices:` references out of
// structure.yaml; everything else under sql/ is treated as transform.

import type { ProfileFileEntry } from "../../../types";
import { cn } from "../../../lib/utils";

type Props = {
  files: ProfileFileEntry[];
  activePath: string;
  onSelect: (path: string) => void;
};

export function FileTree({ files, activePath, onSelect }: Props) {
  const groups = buildFileGroups(files);
  return (
    <nav className="w-[180px] shrink-0 border-r border-neutral-200 overflow-auto py-[6px]">
      {groups.map((group, idx) => {
        if (group.length === 0) return null;
        const hasPrev = groups.slice(0, idx).some((g) => g.length > 0);
        return (
          <div key={idx}>
            {hasPrev && <div className="my-[6px] mx-[10px] border-t border-neutral-200" />}
            {group.map((f) => (
              <FileItem
                key={f.path}
                file={f}
                active={activePath === f.path}
                onSelect={onSelect}
              />
            ))}
          </div>
        );
      })}
    </nav>
  );
}

function FileItem({
  file,
  active,
  onSelect,
}: {
  file: ProfileFileEntry;
  active: boolean;
  onSelect: (p: string) => void;
}) {
  const basename = file.path.split("/").pop() ?? file.path;
  const dot = basename.lastIndexOf(".");
  const stem = dot > 0 ? basename.slice(0, dot) : basename;
  const ext = dot > 0 ? basename.slice(dot + 1) : "";
  return (
    <button
      type="button"
      onClick={() => onSelect(file.path)}
      className={cn(
        "w-full text-left flex items-center justify-between gap-[6px] px-[12px] py-[5px] text-[12px] border-0 transition-colors cursor-pointer",
        active
          ? "bg-neutral-100 text-neutral-900"
          : "bg-transparent text-neutral-700 hover:bg-black/5 hover:text-neutral-900",
      )}
    >
      <span className="truncate">{stem}</span>
      {ext && (
        <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
          {ext}
        </span>
      )}
    </button>
  );
}

function buildFileGroups(files: ProfileFileEntry[]): ProfileFileEntry[][] {
  const yaml = files.find((f) => f.path === "structure.yaml");
  const yamlContent = yaml?.content ?? "";

  const noticeRefs = new Set<string>();
  const blocks = yamlContent.matchAll(
    /notices:\s*\n((?:[ \t]+-[ \t]+[\w./-]+\s*\n?)+)/g,
  );
  for (const block of blocks) {
    const body = block[1] ?? "";
    for (const item of body.matchAll(/-[ \t]+([\w./-]+)/g)) {
      noticeRefs.add(item[1].replace(/^sql\//, ""));
    }
  }

  const profileFiles: ProfileFileEntry[] = [];
  const transformFiles: ProfileFileEntry[] = [];
  const noticeFiles: ProfileFileEntry[] = [];

  for (const f of files) {
    if (f.path === "structure.yaml" || f.path === "instructions.md") {
      profileFiles.push(f);
    } else {
      const name = f.path.split("/").pop() ?? f.path;
      if (f.path.endsWith(".sql") && noticeRefs.has(name)) {
        noticeFiles.push(f);
      } else {
        transformFiles.push(f);
      }
    }
  }

  return [profileFiles, transformFiles, noticeFiles];
}
