// App.tsx
//
// Root component. Owns all shared state. Renders the shell:
// Titlebar on top, then a floating panel containing Sidebar + MainPanel.
//
// Phase 1: mock data only. invoke() is wired up in later steps.

import { useState } from "react";
import type { LoadedProfile, ProfileSummary } from "./types";
import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { MainPanel } from "./components/MainPanel";

export type FileStatus = "none" | "pending" | "valid" | "invalid";
export type GenerateStatus = "idle" | "running" | "done";

// ── Mock data (phase 1) ───────────────────────────────────────────────────────
// Mirrors the real shape from types.ts so wiring later is a straight swap.

const MOCK_PROFILES: ProfileSummary[] = [
  { id: "test1", name: "Simple Import", version: "1.0.0", zip_path: "" },
];

const MOCK_PROFILE: LoadedProfile = {
  structure: {
    id: "test1",
    name: "Simple Import",
    version: "1.0.0",
    min_app_version: "0.1.0",
    inputs: [
      {
        label: "Classification",
        type: "csv",
        required: true,
        validation: [],
      },
    ],
    outputs: [{ label: "Update_Records", type: "csv" }],
    steps: [
      {
        label: "AddSourceFiles",
        type: "file_input",
        input: [{ label: "Classification", validate: true }],
      },
      {
        label: "CreateImportFile",
        type: "sql_transform",
        input: ["Classification"],
        sql: "primary_transform.sql",
        output: ["Update_Records"],
      },
      { label: "Import", type: "manual_instruction" },
    ],
  },
  instructions: {
    _header:
      "# Simple Import\n\nUpdate records in database with new info from vendor.",
    AddSourceFiles:
      "## Select Source Files\n\nUpload the un-edited data file (typically `file_from_vendor.csv`) that the vendor provided.",
    CreateImportFile: "## Generate Import File",
    Import:
      "## Import into database\n\nImport into the database using the `SimpleImport` profile.\n\n![Import tool](assets/import_profile.png)\n\nIf there are exceptions, contact vendor.",
  },
  sql_files: {},
  temp_dir: "",
};

export default function App() {
  // ── State ───────────────────────────────────────────────────────────────────
  const [profiles] = useState<ProfileSummary[]>(MOCK_PROFILES);
  const [selectedProfile, setSelectedProfile] = useState<string | null>("test1");
  const [loadedProfile] = useState<LoadedProfile | null>(MOCK_PROFILE);

  const [filePath, setFilePath] = useState<string | null>(null);
  const [fileName, setFileName] = useState<string | null>(null);
  const [fileStatus, setFileStatus] = useState<FileStatus>("none");

  const [generateStatus, setGenerateStatus] = useState<GenerateStatus>("idle");
  const [generateProgress, setGenerateProgress] = useState<number>(0);

  // ── Derived step completion ────────────────────────────────────────────────
  const stepsDone: Record<string, boolean> = {};
  if (loadedProfile) {
    for (const step of loadedProfile.structure.steps) {
      if (step.type === "file_input") stepsDone[step.label] = fileStatus === "valid";
      else if (step.type === "sql_transform") stepsDone[step.label] = generateStatus === "done";
      else stepsDone[step.label] = false;
    }
  }

  // ── Mock handlers (replaced with invoke() in later phase) ──────────────────
  const handleFileSelect = (path: string, name: string) => {
    setFilePath(path);
    setFileName(name);
    setFileStatus("pending");
    setGenerateStatus("idle");
    setGenerateProgress(0);
  };

  const handleValidate = () => {
    // mock: pretend the file is valid
    setFileStatus("valid");
    setGenerateStatus("idle");
    setGenerateProgress(0);
  };

  const handleGenerate = () => {
    setGenerateStatus("running");
    setGenerateProgress(0);
    // mock progress
    let p = 0;
    const tick = () => {
      p += 20;
      setGenerateProgress(p);
      if (p >= 100) {
        setGenerateStatus("done");
        return;
      }
      setTimeout(tick, 120);
    };
    setTimeout(tick, 120);
  };

  const handleDownload = () => {
    // mock: no-op
  };

  // Resets workflow state for the currently-loaded profile.
  // Keeps profile selection; only clears file + generation state.
  const handleReset = () => {
    setFilePath(null);
    setFileName(null);
    setFileStatus("none");
    setGenerateStatus("idle");
    setGenerateProgress(0);
  };

  // Suppress unused-warning; filePath is used once we wire backend
  void filePath;

  return (
    <div className="flex flex-col h-screen bg-neutral-100 text-neutral-900">
      <Titlebar />
      <div className="flex-1 p-4 pt-0 overflow-hidden">
        <div className="flex h-full bg-white rounded-xl border border-neutral-200 shadow-md overflow-hidden">
          <Sidebar
            profiles={profiles}
            selectedProfile={selectedProfile}
            onSelectProfile={setSelectedProfile}
            loadedProfile={loadedProfile}
            stepsDone={stepsDone}
            onReset={handleReset}
          />
          <MainPanel
            loadedProfile={loadedProfile}
            stepsDone={stepsDone}
            fileName={fileName}
            fileStatus={fileStatus}
            generateStatus={generateStatus}
            generateProgress={generateProgress}
            onFileSelect={handleFileSelect}
            onValidate={handleValidate}
            onGenerate={handleGenerate}
            onDownload={handleDownload}
          />
        </div>
      </div>
    </div>
  );
}
