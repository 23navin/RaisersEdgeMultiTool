// Shared types local to the settings panel.

import type { ProfileSummary, ProfileFileEntry } from "../../../types";

// The pair of "which profile + which files" that the editor mutates.
// Separate from `LoadedProfile` because the editor only needs the summary
// metadata + the raw file list, not the parsed structure.
export type EditorState = {
  summary: ProfileSummary;
  files: ProfileFileEntry[];
};
