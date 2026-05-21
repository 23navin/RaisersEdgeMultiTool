// ImportsPage.tsx
//
// The Imports tab: profile sidebar on the left, step content on the right.

import type { LoadedProfile, ProfileSummary } from "../../types";
import type { FileEntry, GenEntry } from "../../App";
import { Sidebar } from "./Sidebar";
import { MainPanel } from "./MainPanel";

type ImportsPageProps = {
  profiles: ProfileSummary[];
  selectedProfile: string | null;
  onSelectProfile: (zipPath: string) => void;
  loadedProfile: LoadedProfile | null;
  stepsDone: Record<string, boolean>;
  files: Record<string, FileEntry>;
  generations: Record<string, GenEntry>;
  onFileSelect: (inputLabel: string, path: string, name: string) => void;
  onValidate: (inputLabel: string) => void;
  onClearFile: (inputLabel: string) => void;
  onGenerate: (stepLabel: string, transformIdx: number) => void;
  onDownload: (stepLabel: string, transformIdx: number, outputLabel: string) => void;
  onReset: () => void;
};

export function ImportsPage({
  profiles,
  selectedProfile,
  onSelectProfile,
  loadedProfile,
  stepsDone,
  files,
  generations,
  onFileSelect,
  onValidate,
  onClearFile,
  onGenerate,
  onDownload,
  onReset,
}: ImportsPageProps) {
  return (
    <>
      <Sidebar
        profiles={profiles}
        selectedProfile={selectedProfile}
        onSelectProfile={onSelectProfile}
        loadedProfile={loadedProfile}
        stepsDone={stepsDone}
        onReset={onReset}
      />
      <MainPanel
        loadedProfile={loadedProfile}
        stepsDone={stepsDone}
        files={files}
        generations={generations}
        onFileSelect={onFileSelect}
        onValidate={onValidate}
        onClearFile={onClearFile}
        onGenerate={onGenerate}
        onDownload={onDownload}
      />
    </>
  );
}
