import type { ProfileFileEntry } from "../../../types";

// structure.yaml first, then instructions.md, then everything else alphabetically.
export function sortProfileFiles(files: ProfileFileEntry[]): ProfileFileEntry[] {
  const rank = (p: string) => {
    if (p === "structure.yaml") return 0;
    if (p === "instructions.md") return 1;
    return 2;
  };
  return [...files].sort((a, b) => {
    const r = rank(a.path) - rank(b.path);
    return r !== 0 ? r : a.path.localeCompare(b.path);
  });
}
