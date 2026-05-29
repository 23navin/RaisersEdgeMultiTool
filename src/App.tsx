// App.tsx
//
// Root component. Owns all shared state and is the only place that calls
// invoke(). Renders the shell: Titlebar on top, then a floating panel
// containing Sidebar + MainPanel.

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type {
  LoadedProfile,
  Notice,
  OutputFile,
  ProfileSummary,
  SqlError,
  TransformResult,
  ValidationError,
} from "./types";
import { Titlebar, type TopTab } from "./components/Titlebar";
import { ImportsPage } from "./components/imports/ImportsPage";
import {
  DataRequestsPage,
  DEFAULT_LIBRARY_RATIO,
  type Mode as DataReqMode,
} from "./components/data-request/DataRequestsPage";
import { ReportsPage } from "./components/reports/ReportsPage";
import { SettingsPanel } from "./components/SettingsPanel";
import {
  PanelTransition,
  PANEL_TRANSITION_MS,
} from "./components/PanelTransition";
import { refLabel, stepTransforms } from "./lib/profile-utils";

const TAB_ORDER: TopTab[] = ["imports", "data-requests", "reports"];

export type FileStatus = "none" | "pending" | "valid" | "invalid";
export type GenerateStatus = "idle" | "running" | "done" | "error";

export type FileEntry = {
  path: string;
  name: string;
  status: FileStatus;
  errors?: ValidationError[];
  notices?: Notice[];
};
export type GenEntry = {
  status: GenerateStatus;
  progress: number;
  errors?: SqlError[];
  notices?: Notice[];
  outputs?: OutputFile[];
};

// Composite key for a single transform within a sql_transform step.
function genKey(stepLabel: string, transformIdx: number): string {
  return `${stepLabel}::${transformIdx}`;
}

function asString(e: unknown): string {
  return typeof e === "string" ? e : String(e);
}

// Shape returned by the validate_file backend command.
type ValidationResult = {
  ok: boolean;
  errors: ValidationError[];
  notices: Notice[];
};

export default function App() {
  // ── State ───────────────────────────────────────────────────────────────────
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [loadedProfile, setLoadedProfile] = useState<LoadedProfile | null>(null);
  const [files, setFiles] = useState<Record<string, FileEntry>>({});
  const [generations, setGenerations] = useState<Record<string, GenEntry>>({});
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<TopTab>("imports");
  const [exitingTab, setExitingTab] = useState<TopTab | null>(null);
  const [transitionDir, setTransitionDir] = useState<"left" | "right">("left");

  // Data Requests panel layout — held here so it survives the page
  // unmounting when the user navigates to another tab and back.
  const [dataReqMode, setDataReqMode] = useState<DataReqMode>("default");
  const [dataReqRatio, setDataReqRatio] = useState<number>(DEFAULT_LIBRARY_RATIO);

  const handleTabChange = (newTab: TopTab) => {
    if (newTab === activeTab || exitingTab) return;
    const oldIdx = TAB_ORDER.indexOf(activeTab);
    const newIdx = TAB_ORDER.indexOf(newTab);
    setTransitionDir(newIdx > oldIdx ? "left" : "right");
    setExitingTab(activeTab);
    setActiveTab(newTab);
  };

  useEffect(() => {
    if (!exitingTab) return;
    const t = setTimeout(() => setExitingTab(null), PANEL_TRANSITION_MS + 20);
    return () => clearTimeout(t);
  }, [exitingTab]);

  // Increments on every load_profile call so late-arriving responses for a
  // profile the user has already navigated away from get discarded.
  const loadRequestId = useRef(0);

  // Load the profile list once on mount.
  useEffect(() => {
    (async () => {
      try {
        const list = await invoke<ProfileSummary[]>("list_profiles");
        setProfiles(list);
      } catch (e) {
        console.error("list_profiles failed:", e);
      }
    })();
  }, []);

  // ── Derived step completion ────────────────────────────────────────────────
  const stepsDone: Record<string, boolean> = {};
  if (loadedProfile) {
    for (const step of loadedProfile.structure.steps) {
      if (step.type === "file_input") {
        const inputs = step.input ?? [];
        stepsDone[step.label] =
          inputs.length > 0 &&
          inputs.every((r) => files[refLabel(r)]?.status === "valid");
      } else if (step.type === "sql_transform") {
        const transforms = stepTransforms(step);
        stepsDone[step.label] = transforms.every(
          (_, i) => generations[genKey(step.label, i)]?.status === "done"
        );
      } else {
        stepsDone[step.label] = false;
      }
    }
  }

  // Returns the (stepLabel, transformIdx) keys for every transform that
  // consumes the given input.
  const transformsConsumingInput = (inputLabel: string): string[] => {
    if (!loadedProfile) return [];
    const keys: string[] = [];
    for (const step of loadedProfile.structure.steps) {
      if (step.type !== "sql_transform") continue;
      stepTransforms(step).forEach((t, idx) => {
        if ((t.input ?? []).some((r) => refLabel(r) === inputLabel)) {
          keys.push(genKey(step.label, idx));
        }
      });
    }
    return keys;
  };

  const resetGenerationsConsuming = (inputLabel: string) => {
    const affected = transformsConsumingInput(inputLabel);
    if (!affected.length) return;
    setGenerations((prev) => {
      const next = { ...prev };
      for (const k of affected) delete next[k];
      return next;
    });
  };

  // ── Handlers ──────────────────────────────────────────────────────────────

  const handleFileSelect = (inputLabel: string, path: string, name: string) => {
    setFiles((prev) => ({
      ...prev,
      [inputLabel]: { path, name, status: "pending" },
    }));
    resetGenerationsConsuming(inputLabel);
  };

  const handleValidate = async (inputLabel: string) => {
    const file = files[inputLabel];
    if (!file || !loadedProfile) return;
    try {
      const result = await invoke<ValidationResult>("validate_file", {
        filePath: file.path,
        inputLabel,
        zipPath: loadedProfile.temp_dir,
      });
      setFiles((prev) => {
        const cur = prev[inputLabel];
        if (!cur) return prev;
        return {
          ...prev,
          [inputLabel]: {
            ...cur,
            status: result.ok ? "valid" : "invalid",
            errors: result.ok ? undefined : result.errors,
            notices: result.notices?.length ? result.notices : undefined,
          },
        };
      });
    } catch (e) {
      console.error("validate_file failed:", e);
      setFiles((prev) => {
        const cur = prev[inputLabel];
        if (!cur) return prev;
        return {
          ...prev,
          [inputLabel]: {
            ...cur,
            status: "invalid",
            errors: [{ message: asString(e) }],
            notices: undefined,
          },
        };
      });
    }
    resetGenerationsConsuming(inputLabel);
  };

  const handleClearFile = (inputLabel: string) => {
    setFiles((prev) => {
      if (!(inputLabel in prev)) return prev;
      const next = { ...prev };
      delete next[inputLabel];
      return next;
    });
    resetGenerationsConsuming(inputLabel);
  };

  const handleGenerate = async (stepLabel: string, transformIdx: number) => {
    if (!loadedProfile) return;
    const key = genKey(stepLabel, transformIdx);

    const step = loadedProfile.structure.steps.find((s) => s.label === stepLabel);
    const transforms = step ? stepTransforms(step) : [];
    const transform = transforms[transformIdx];
    if (!transform) return;

    // Collect the uploaded file path for every input the transform consumes.
    const filePaths: Record<string, string> = {};
    for (const ref of transform.input ?? []) {
      const lbl = refLabel(ref);
      const f = files[lbl];
      if (f?.status === "valid") filePaths[lbl] = f.path;
    }

    setGenerations((prev) => ({
      ...prev,
      [key]: { status: "running", progress: 0 },
    }));

    try {
      const result = await invoke<TransformResult>("run_profile", {
        filePaths,
        sqlFile: transform.sql,
        zipPath: loadedProfile.temp_dir,
        outputLabels: transform.output ?? [],
      });
      setGenerations((prev) => ({
        ...prev,
        [key]: {
          status: "done",
          progress: 100,
          outputs: result.outputs,
          notices: result.notices,
        },
      }));
    } catch (e) {
      console.error("run_profile failed:", e);
      setGenerations((prev) => ({
        ...prev,
        [key]: {
          status: "error",
          progress: 100,
          errors: [{ errorType: "Error", message: asString(e) }],
        },
      }));
    }
  };

  const handleDownload = async (
    stepLabel: string,
    transformIdx: number,
    outputLabel: string,
  ) => {
    const key = genKey(stepLabel, transformIdx);
    const output = generations[key]?.outputs?.find((o) => o.label === outputLabel);
    if (!output) return;
    try {
      const dest = await save({
        defaultPath: `${outputLabel}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (typeof dest !== "string") return; // user cancelled
      await invoke<void>("save_output", {
        srcPath: output.path,
        destPath: dest,
      });
    } catch (e) {
      console.error("save_output failed:", e);
    }
  };

  // Resets workflow state for the currently-loaded profile.
  // Keeps profile selection; only clears file + generation state.
  const handleReset = () => {
    setFiles({});
    setGenerations({});
  };

  // `zipPath` is the unique selection key — built-in and user profiles can
  // share an `id`, so we discriminate by zip_path (which is always unique:
  // either a real fs path or a "builtin://<filename>" sentinel).
  const handleSelectProfile = async (zipPath: string | null) => {
    const reqId = ++loadRequestId.current;
    setSelectedProfile(zipPath);
    setFiles({});
    setGenerations({});
    setLoadedProfile(null);
    if (zipPath == null) return;
    const summary = profiles.find((p) => p.zip_path === zipPath);
    if (!summary) return;
    try {
      const loaded = await invoke<LoadedProfile>("load_profile", {
        zipPath: summary.zip_path,
      });
      if (loadRequestId.current === reqId) setLoadedProfile(loaded);
    } catch (e) {
      if (loadRequestId.current !== reqId) return;
      console.error("load_profile failed:", e);
      setSelectedProfile(null);
    }
  };

  const renderPanel = (tab: TopTab) => {
    if (tab === "imports") {
      return (
        <ImportsPage
          profiles={profiles}
          selectedProfile={selectedProfile}
          onSelectProfile={handleSelectProfile}
          loadedProfile={loadedProfile}
          stepsDone={stepsDone}
          files={files}
          generations={generations}
          onFileSelect={handleFileSelect}
          onValidate={handleValidate}
          onClearFile={handleClearFile}
          onGenerate={handleGenerate}
          onDownload={handleDownload}
          onReset={handleReset}
        />
      );
    }
    if (tab === "data-requests")
      return (
        <DataRequestsPage
          mode={dataReqMode}
          setMode={setDataReqMode}
          customRatio={dataReqRatio}
          setCustomRatio={setDataReqRatio}
        />
      );
    return <ReportsPage />;
  };

  return (
    <div className="flex flex-col h-screen bg-neutral-100 text-neutral-900">
      <Titlebar
        activeTab={activeTab}
        onTabChange={handleTabChange}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <div className="flex-1 p-4 pt-0 overflow-hidden">
        <div className="relative h-full">
          {exitingTab && (
            <PanelTransition
              key={`exit-${exitingTab}`}
              state="exit"
              direction={transitionDir}
            >
              {renderPanel(exitingTab)}
            </PanelTransition>
          )}
          <PanelTransition
            key={`active-${activeTab}`}
            state={exitingTab ? "enter" : "idle"}
            direction={transitionDir}
          >
            {renderPanel(activeTab)}
          </PanelTransition>
          <SettingsPanel
            open={settingsOpen}
            onClose={() => {
              setSettingsOpen(false);
              // Refresh the main sidebar's profile list — the user may have
              // created, duplicated, or deleted a profile while the panel
              // was open. Clear selection if it's no longer on disk.
              invoke<ProfileSummary[]>("list_profiles")
                .then((list) => {
                  setProfiles(list);
                  if (
                    selectedProfile &&
                    !list.some((p) => p.zip_path === selectedProfile)
                  ) {
                    setSelectedProfile(null);
                    setLoadedProfile(null);
                  }
                })
                .catch((e) => console.error("list_profiles failed:", e));
            }}
          />
        </div>
      </div>
    </div>
  );
}
