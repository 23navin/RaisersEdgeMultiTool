// App.tsx
//
// Root component. Owns all shared state and calls the backend through
// lib/api.ts (the transport module — invoke() on desktop, fetch() on the
// web). Renders the shell: Titlebar on top, then a floating panel
// containing Sidebar + MainPanel.

import { useEffect, useRef, useState } from "react";
import * as api from "./lib/api";
import type {
  LoadedProfile,
  Notice,
  OutputFile,
  ProfileStructure,
  ProfileSummary,
  Parameter,
  ReportRunResult,
  QueryStepResult,
  ResultSet,
  SqlError,
  SyncResult,
  ValidationError,
} from "./types";
import { Titlebar, type TopTab } from "./components/Titlebar";
import { ImportsPage } from "./components/imports/ImportsPage";
import {
  DataRequestsPage,
  DEFAULT_LIBRARY_RATIO,
  type Mode as DataReqMode,
} from "./components/data-request/DataRequestsPage";
import {
  ReportsPage,
  type ReportStatus,
  type ActionState,
} from "./components/reports/ReportsPage";
import { SettingsPanel } from "./components/SettingsPanel";
import {
  PanelTransition,
  PANEL_TRANSITION_MS,
} from "./components/PanelTransition";
import { refLabel, stepTransforms } from "./lib/profile-utils";

const TAB_ORDER: TopTab[] = ["imports", "data-requests", "reports"];

export type FileStatus = "none" | "pending" | "valid" | "invalid";
export type GenerateStatus = "idle" | "running" | "done" | "error";

// A code_table_sync step runs in one shot — no progress bar, so no separate
// progress field the way a generation has.
export type SyncStatus = "idle" | "running" | "done" | "error";

// An re_query step likewise. The SKY job reports no progress, so the UI shows an
// indeterminate bar rather than a percentage.
export type QueryStatus = "idle" | "running" | "done" | "error";

// A visualization step likewise — one local DuckDB SELECT, no progress to report.
export type VizStatus = "idle" | "running" | "done" | "error";

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

// One code_table_sync step's run state, keyed by step label (a sync step holds
// exactly one operation, so it needs no composite key the way transforms do).
export type SyncEntry = {
  status: SyncStatus;
  result?: SyncResult;
  error?: string;
};

// One re_query step's run state, keyed by step label. `result.artifact_id` is what gets
// handed to whichever later transform declared this step's query_output.
export type QueryEntry = {
  status: QueryStatus;
  result?: QueryStepResult;
  error?: string;
};

// One visualization step's run state, keyed by step label. `data` is the rows
// the step's SELECT returned, handed straight to the viz component to draw.
export type VizEntry = {
  status: VizStatus;
  data?: ResultSet;
  error?: string;
};

// Composite key for a single transform within a sql_transform step.
function genKey(stepLabel: string, transformIdx: number): string {
  return `${stepLabel}::${transformIdx}`;
}

function asString(e: unknown): string {
  return typeof e === "string" ? e : String(e);
}

function isoDate(d: Date): string {
  return d.toISOString().slice(0, 10);
}

// Resolve a parameter's declared default into a concrete control value.
function resolveParamDefault(p: Parameter): unknown {
  const d = p.default as
    | { preset?: string; from?: string; to?: string }
    | string
    | number
    | undefined;
  if (p.type === "date_range") {
    const obj = (d ?? {}) as { preset?: string; from?: string; to?: string };
    if (obj.preset === "last_30_days") {
      const to = new Date();
      const from = new Date();
      from.setDate(from.getDate() - 30);
      return { from: isoDate(from), to: isoDate(to) };
    }
    if (obj.from || obj.to) return { from: obj.from, to: obj.to };
    return {};
  }
  if (d != null && typeof d !== "object") return String(d);
  return "";
}

function initialParamValues(structure: ProfileStructure): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const p of structure.parameters) out[p.id] = resolveParamDefault(p);
  return out;
}

export default function App() {
  // ── State ───────────────────────────────────────────────────────────────────
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [loadedProfile, setLoadedProfile] = useState<LoadedProfile | null>(null);
  const [files, setFiles] = useState<Record<string, FileEntry>>({});
  const [generations, setGenerations] = useState<Record<string, GenEntry>>({});
  const [syncs, setSyncs] = useState<Record<string, SyncEntry>>({});
  const [queries, setQueries] = useState<Record<string, QueryEntry>>({});
  const [visualizations, setVisualizations] = useState<Record<string, VizEntry>>({});
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<TopTab>("imports");
  const [exitingTab, setExitingTab] = useState<TopTab | null>(null);
  const [transitionDir, setTransitionDir] = useState<"left" | "right">("left");

  // Data Requests panel layout — held here so it survives the page
  // unmounting when the user navigates to another tab and back.
  const [dataReqMode, setDataReqMode] = useState<DataReqMode>("default");
  const [dataReqRatio, setDataReqRatio] = useState<number>(DEFAULT_LIBRARY_RATIO);

  // ── Reports state ─────────────────────────────────────────────────────────
  const [selectedReport, setSelectedReport] = useState<string | null>(null);
  const [loadedReport, setLoadedReport] = useState<LoadedProfile | null>(null);
  const [reportParams, setReportParams] = useState<Record<string, unknown>>({});
  const [reportRun, setReportRun] = useState<ReportRunResult | null>(null);
  const [reportStatus, setReportStatus] = useState<ReportStatus>("idle");
  const [reportError, setReportError] = useState<string | null>(null);
  const [reportStale, setReportStale] = useState(false);
  const [actionStates, setActionStates] = useState<Record<string, ActionState>>({});
  const reportLoadId = useRef(0);

  // Built-in + user profiles split by kind. Reports go to the Reports tab;
  // everything else stays in Imports.
  const reportProfiles = profiles.filter((p) => p.kind === "report");
  const importProfiles = profiles.filter((p) => p.kind !== "report");

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

  // Returning from a web OAuth handshake, reveal Settings so the outcome —
  // connected, or the error the callback reported — is actually seen. Without
  // this the redirect lands on a normal-looking page and the result is
  // consumed invisibly by the always-mounted settings panel.
  useEffect(() => {
    if (api.hasConnectResult()) setSettingsOpen(true);
  }, []);

  // Load the profile list once on mount.
  useEffect(() => {
    (async () => {
      try {
        const list = await api.listProfiles();
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
      } else if (step.type === "re_query") {
        stepsDone[step.label] = queries[step.label]?.status === "done";
      } else if (step.type === "code_table_sync") {
        const st = syncs[step.label];
        stepsDone[step.label] = st?.status === "done" && (st.result?.ok ?? false);
      } else if (step.type === "visualization") {
        stepsDone[step.label] = visualizations[step.label]?.status === "done";
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

  // The fields a transform uses to declare an upstream step's result.
  type UpstreamField = "query_input" | "sync_input";

  // Transform keys that read a given upstream result. One walk parameterised by
  // the declaring field, so adding a third upstream family doesn't need another
  // copy of this.
  const transformsConsuming = (field: UpstreamField, label: string): string[] => {
    if (!loadedProfile) return [];
    const keys: string[] = [];
    for (const step of loadedProfile.structure.steps) {
      if (step.type !== "sql_transform") continue;
      stepTransforms(step).forEach((t, idx) => {
        if ((t[field] ?? []).includes(label)) {
          keys.push(genKey(step.label, idx));
        }
      });
    }
    return keys;
  };

  // Visualization steps that read a given upstream result. Keyed by step label
  // rather than a composite key — a visualization step holds one result set.
  const visualizationsConsuming = (field: UpstreamField, label: string): string[] => {
    if (!loadedProfile) return [];
    return loadedProfile.structure.steps
      .filter((s) => s.type === "visualization" && (s[field] ?? []).includes(label))
      .map((s) => s.label);
  };

  // Clears every transform and visualization that reads this upstream result.
  // Called when the producing step re-runs and when it's invalidated — a stale
  // join, or a stale table on screen, is worse than a missing one.
  const resetTransformsConsuming = (field: UpstreamField, label: string) => {
    const affected = transformsConsuming(field, label);
    if (affected.length) {
      setGenerations((prev) => {
        const next = { ...prev };
        for (const k of affected) delete next[k];
        return next;
      });
    }
    const affectedViz = visualizationsConsuming(field, label);
    if (affectedViz.length) {
      setVisualizations((prev) => {
        const next = { ...prev };
        for (const k of affectedViz) delete next[k];
        return next;
      });
    }
  };

  // Changing an input invalidates, in order: transforms that read it, the
  // re_query and code_table_sync steps that read it, and — because those
  // results are now stale — the transforms downstream of them.
  const resetGenerationsConsuming = (inputLabel: string) => {
    const affected = transformsConsumingInput(inputLabel);
    if (affected.length) {
      setGenerations((prev) => {
        const next = { ...prev };
        for (const k of affected) delete next[k];
        return next;
      });
    }
    if (!loadedProfile) return;
    for (const step of loadedProfile.structure.steps) {
      const readsIt = (step.input ?? []).some((r) => refLabel(r) === inputLabel);
      if (!readsIt) continue;

      if (step.type === "re_query") {
        setQueries((prev) => {
          if (!prev[step.label]) return prev;
          const next = { ...prev };
          delete next[step.label];
          return next;
        });
        if (step.query_output) {
          resetTransformsConsuming("query_input", step.query_output);
        }
      } else if (step.type === "visualization") {
        setVisualizations((prev) => {
          if (!prev[step.label]) return prev;
          const next = { ...prev };
          delete next[step.label];
          return next;
        });
      } else if (step.type === "code_table_sync") {
        // The rows already pushed to RE can't be un-pushed, but the recorded
        // outcome no longer describes the file on screen, so it stops counting
        // as done and anything joined to it is cleared.
        setSyncs((prev) => {
          if (!prev[step.label]) return prev;
          const next = { ...prev };
          delete next[step.label];
          return next;
        });
        if (step.sync_output) {
          resetTransformsConsuming("sync_input", step.sync_output);
        }
      }
    }
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
      const result = await api.validateFile(
        file.path,
        inputLabel,
        loadedProfile.session_id,
      );
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

    // Query results this transform declared via query_input. A declared label
    // whose step hasn't run yet is simply absent — the backend leaves the
    // placeholder unsubstituted and DuckDB reports it, rather than us guessing.
    const queryIds: Record<string, string> = {};
    for (const label of transform.query_input ?? []) {
      const producer = loadedProfile.structure.steps.find(
        (s) => s.type === "re_query" && s.query_output === label,
      );
      const res = producer ? queries[producer.label]?.result : undefined;
      if (res) queryIds[label] = res.artifact_id;
    }

    // Same contract for code_table_sync outcomes declared via sync_input. A
    // sync that hasn't run yet is absent for the same reason: DuckDB naming the
    // unresolved placeholder beats us inventing an empty table.
    const syncIds: Record<string, string> = {};
    for (const label of transform.sync_input ?? []) {
      const producer = loadedProfile.structure.steps.find(
        (s) => s.type === "code_table_sync" && s.sync_output === label,
      );
      const res = producer ? syncs[producer.label]?.result : undefined;
      if (res?.artifact_id) syncIds[label] = res.artifact_id;
    }

    setGenerations((prev) => ({
      ...prev,
      [key]: { status: "running", progress: 0 },
    }));

    try {
      const result = await api.runProfile({
        filePaths,
        queryIds,
        syncIds,
        sqlFile: transform.sql,
        sessionId: loadedProfile.session_id,
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

  // Runs an re_query step: the backend executes the step's params SQL over the
  // uploaded files, substitutes the values into the query template, calls RE,
  // and writes the rows to a temp file. The returned path is what downstream
  // transforms read as {{query:Label}}.
  const handleRunQuery = async (stepLabel: string) => {
    if (!loadedProfile) return;
    const step = loadedProfile.structure.steps.find((s) => s.label === stepLabel);
    if (!step) return;

    const filePaths: Record<string, string> = {};
    for (const ref of step.input ?? []) {
      const lbl = refLabel(ref);
      const f = files[lbl];
      if (f?.status === "valid") filePaths[lbl] = f.path;
    }

    setQueries((prev) => ({ ...prev, [stepLabel]: { status: "running" } }));
    // Anything already built off the previous result is now stale.
    if (step.query_output) resetTransformsConsuming("query_input", step.query_output);

    try {
      const result = await api.runReQuery(filePaths, stepLabel, loadedProfile.session_id);
      setQueries((prev) => ({
        ...prev,
        [stepLabel]: { status: "done", result },
      }));
    } catch (e) {
      console.error("run_re_query failed:", e);
      setQueries((prev) => ({
        ...prev,
        [stepLabel]: { status: "error", error: asString(e) },
      }));
    }
  };

  // Runs a code_table_sync step: the backend executes the step's SQL over the
  // uploaded files and pushes one write per row to RE. Failures come back
  // per-row rather than aborting, so a partial result is still reported.
  const handleCodeTableSync = async (stepLabel: string) => {
    if (!loadedProfile) return;
    const step = loadedProfile.structure.steps.find((s) => s.label === stepLabel);
    if (!step) return;

    const filePaths: Record<string, string> = {};
    for (const ref of step.input ?? []) {
      const lbl = refLabel(ref);
      const f = files[lbl];
      if (f?.status === "valid") filePaths[lbl] = f.path;
    }

    setSyncs((prev) => ({ ...prev, [stepLabel]: { status: "running" } }));
    // Anything joined to the previous run's outcome rows is now stale.
    if (step.sync_output) resetTransformsConsuming("sync_input", step.sync_output);
    try {
      const result = await api.runCodeTableSync(
        filePaths,
        stepLabel,
        loadedProfile.session_id,
      );
      setSyncs((prev) => ({
        ...prev,
        [stepLabel]: { status: "done", result },
      }));
    } catch (e) {
      console.error("run_code_table_sync failed:", e);
      setSyncs((prev) => ({
        ...prev,
        [stepLabel]: { status: "error", error: asString(e) },
      }));
    }
  };

  // Runs a visualization step: the backend executes the step's SELECT over the
  // uploaded files and whatever upstream results the step declares, and returns
  // the rows. Nothing is written and nothing is sent to RE — this only re-reads
  // what earlier steps already produced.
  const handleRunVisualization = async (stepLabel: string) => {
    if (!loadedProfile) return;
    const step = loadedProfile.structure.steps.find((s) => s.label === stepLabel);
    if (!step) return;

    const filePaths: Record<string, string> = {};
    for (const ref of step.input ?? []) {
      const lbl = refLabel(ref);
      const f = files[lbl];
      if (f?.status === "valid") filePaths[lbl] = f.path;
    }

    // Same contract as a transform: a declared label whose producing step
    // hasn't run is simply absent, and DuckDB names the unresolved placeholder.
    const queryIds: Record<string, string> = {};
    for (const label of step.query_input ?? []) {
      const producer = loadedProfile.structure.steps.find(
        (s) => s.type === "re_query" && s.query_output === label,
      );
      const res = producer ? queries[producer.label]?.result : undefined;
      if (res) queryIds[label] = res.artifact_id;
    }

    const syncIds: Record<string, string> = {};
    for (const label of step.sync_input ?? []) {
      const producer = loadedProfile.structure.steps.find(
        (s) => s.type === "code_table_sync" && s.sync_output === label,
      );
      const res = producer ? syncs[producer.label]?.result : undefined;
      if (res?.artifact_id) syncIds[label] = res.artifact_id;
    }

    setVisualizations((prev) => ({ ...prev, [stepLabel]: { status: "running" } }));

    try {
      const data = await api.runVisualization({
        filePaths,
        queryIds,
        syncIds,
        stepLabel,
        sessionId: loadedProfile.session_id,
      });
      setVisualizations((prev) => ({
        ...prev,
        [stepLabel]: { status: "done", data },
      }));
    } catch (e) {
      console.error("run_visualization failed:", e);
      setVisualizations((prev) => ({
        ...prev,
        [stepLabel]: { status: "error", error: asString(e) },
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
    if (!output || !loadedProfile) return;
    try {
      await api.saveOutputFile(
        loadedProfile.session_id,
        output.artifact_id,
        `${outputLabel}.csv`,
      );
    } catch (e) {
      console.error("save_output failed:", e);
    }
  };

  // Resets workflow state for the currently-loaded profile.
  // Keeps profile selection; only clears file + generation state.
  const handleReset = () => {
    setFiles({});
    setGenerations({});
    setSyncs({});
    setQueries({});
    setVisualizations({});
  };

  // `zipPath` is the unique selection key — built-in and user profiles can
  // share an `id`, so we discriminate by zip_path (which is always unique:
  // either a real fs path or a "builtin://<filename>" sentinel).
  const handleSelectProfile = async (zipPath: string | null) => {
    const reqId = ++loadRequestId.current;
    setSelectedProfile(zipPath);
    setFiles({});
    setGenerations({});
    setSyncs({});
    setQueries({});
    setVisualizations({});
    setLoadedProfile(null);
    if (zipPath == null) return;
    const summary = profiles.find((p) => p.zip_path === zipPath);
    if (!summary) return;
    try {
      const loaded = await api.loadProfile(summary.zip_path);
      if (loadRequestId.current === reqId) setLoadedProfile(loaded);
    } catch (e) {
      if (loadRequestId.current !== reqId) return;
      console.error("load_profile failed:", e);
      setSelectedProfile(null);
    }
  };

  // ── Report handlers ───────────────────────────────────────────────────────

  const handleSelectReport = async (zipPath: string) => {
    const reqId = ++reportLoadId.current;
    setSelectedReport(zipPath);
    setLoadedReport(null);
    setReportRun(null);
    setReportStatus("idle");
    setReportError(null);
    setReportStale(false);
    setActionStates({});
    setReportParams({});
    try {
      const loaded = await api.loadProfile(zipPath);
      if (reportLoadId.current !== reqId) return;
      setLoadedReport(loaded);
      setReportParams(initialParamValues(loaded.structure));
    } catch (e) {
      if (reportLoadId.current !== reqId) return;
      console.error("load_profile (report) failed:", e);
      setSelectedReport(null);
    }
  };

  const handleParamChange = (id: string, value: unknown) => {
    setReportParams((prev) => ({ ...prev, [id]: value }));
    // Existing results no longer reflect the inputs until the next refresh.
    setReportStale(true);
  };

  const handleRefresh = async () => {
    if (!loadedReport) return;
    setReportStatus("running");
    setReportError(null);
    try {
      const result = await api.runReport(loadedReport.session_id, reportParams);
      setReportRun(result);
      setReportStatus("done");
      setReportStale(false);
    } catch (e) {
      console.error("run_report failed:", e);
      setReportStatus("error");
      setReportError(asString(e));
    }
  };

  const handleRunAction = async (actionId: string) => {
    if (!loadedReport) return;
    setActionStates((prev) => ({
      ...prev,
      [actionId]: { status: "running" },
    }));
    try {
      const result = await api.runReportAction(
        loadedReport.session_id,
        actionId,
        reportParams,
      );
      setActionStates((prev) => ({
        ...prev,
        [actionId]: {
          status: result.ok ? "done" : "error",
          message: result.message,
        },
      }));
    } catch (e) {
      console.error("run_report_action failed:", e);
      setActionStates((prev) => ({
        ...prev,
        [actionId]: { status: "error", message: asString(e) },
      }));
    }
  };

  const renderPanel = (tab: TopTab) => {
    if (tab === "imports") {
      return (
        <ImportsPage
          profiles={importProfiles}
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
          syncs={syncs}
          onCodeTableSync={handleCodeTableSync}
          queries={queries}
          onRunQuery={handleRunQuery}
          visualizations={visualizations}
          onRunVisualization={handleRunVisualization}
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
    return (
      <ReportsPage
        reports={reportProfiles}
        selectedReport={selectedReport}
        onSelectReport={handleSelectReport}
        loadedReport={loadedReport}
        paramValues={reportParams}
        onParamChange={handleParamChange}
        run={reportRun}
        status={reportStatus}
        error={reportError}
        stale={reportStale}
        onRefresh={handleRefresh}
        actionStates={actionStates}
        onRunAction={handleRunAction}
      />
    );
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
              api.listProfiles()
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
