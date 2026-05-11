// App.tsx
//
// Root component. Owns all shared state and is the only place
// that calls invoke() directly. Child components receive data
// and callbacks as props — they never call invoke() themselves.
//
// State flow:
//   1. On mount → list_profiles → populate profile dropdown
//   2. User selects profile → load_profile → populate steps + instructions
//   3. User attaches files per step → stored in attachedFiles
//   4. User advances through steps → currentStepIndex increments
//   5. On sql_transform step → run_profile → store output path

import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  AppState,
  ProfileSummary,
  LoadedProfile,
  FileAttachment,
  StepStatus,
} from "./types";
import { ProfilePicker } from "./components/ProfilePicker";
import { StepPanel } from "./components/StepPanel";
import { InstructionsPanel } from "./components/InstructionsPanel";

// ── Initial state ─────────────────────────────────────────────────────────────

const initialState: AppState = {
  profiles: [],
  selectedProfile: null,
  loadedProfile: null,
  currentStepIndex: 0,
  stepStatuses: {},
  attachedFiles: {},
  outputPaths: {},
};

// ── App ───────────────────────────────────────────────────────────────────────

export default function App() {
  const [state, setState] = useState<AppState>(initialState);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // ── Load profile list on mount ──────────────────────────────────────────────
  // Asks Rust to scan the profiles/ folder and return summaries.
  // profilesDir is resolved relative to the app binary in production.
  // During development we use an absolute path via the env variable
  // that Tauri sets, or fall back to a hardcoded dev path.

  useEffect(() => {
    invoke<ProfileSummary[]>("list_profiles")
    .then((profiles) => {
      setState((s) => ({ ...s, profiles }));
    })
    .catch((e) => setError(`Failed to load profiles: ${e}`));
  }, []);

  // ── Select a profile ────────────────────────────────────────────────────────
  // Called when user picks from the dropdown.
  // Fully loads the profile bundle and resets workflow state.

  const handleProfileSelect = useCallback(async (profile: ProfileSummary) => {
    setLoading(true);
    setError(null);
    try {
      const loaded = await invoke<LoadedProfile>("load_profile", {
        zipPath: profile.zip_path,
      });

      // Build initial step statuses — first step active, rest pending
      const stepStatuses: Record<string, StepStatus> = {};
      loaded.structure.steps.forEach((step, i) => {
        stepStatuses[step.label] = i === 0 ? "active" : "pending";
      });

      setState({
        ...initialState,
        profiles: state.profiles,
        selectedProfile: profile,
        loadedProfile: loaded,
        currentStepIndex: 0,
        stepStatuses,
        attachedFiles: {},
        outputPaths: {},
      });
    } catch (e) {
      setError(`Failed to load profile: ${e}`);
    } finally {
      setLoading(false);
    }
  }, [state.profiles]);

  // ── Attach a file to an input ───────────────────────────────────────────────
  // Called by StepPanel when the user selects a file for a given input label.
  // Stores the file path and marks it unvalidated — validation happens separately.

  const handleFileAttach = useCallback((inputLabel: string, filePath: string) => {
    const attachment: FileAttachment = {
      inputLabel,
      filePath,
      validated: false,
      validationErrors: [],
    };
    setState((s) => ({
      ...s,
      attachedFiles: { ...s.attachedFiles, [inputLabel]: attachment },
    }));
  }, []);

  // ── Advance to next step ────────────────────────────────────────────────────
  // Marks current step complete, activates the next one.

  const handleStepComplete = useCallback((stepLabel: string) => {
    setState((s) => {
      if (!s.loadedProfile) return s;
      const steps = s.loadedProfile.structure.steps;
      const nextIndex = s.currentStepIndex + 1;
      const nextStep = steps[nextIndex];

      const updatedStatuses = {
        ...s.stepStatuses,
        [stepLabel]: "complete" as StepStatus,
        ...(nextStep ? { [nextStep.label]: "active" as StepStatus } : {}),
      };

      return {
        ...s,
        currentStepIndex: nextIndex,
        stepStatuses: updatedStatuses,
      };
    });
  }, []);

  // ── Mark step errored ───────────────────────────────────────────────────────

  const handleStepError = useCallback((stepLabel: string, message: string) => {
    setState((s) => ({
      ...s,
      stepStatuses: {
        ...s.stepStatuses,
        [stepLabel]: "error" as StepStatus,
      },
    }));
    setError(message);
  }, []);

  // ── Store output path after transform ──────────────────────────────────────

  const handleOutputReady = useCallback((outputLabel: string, filePath: string) => {
    setState((s) => ({
      ...s,
      outputPaths: { ...s.outputPaths, [outputLabel]: filePath },
    }));
  }, []);

  // ── Derive current step ─────────────────────────────────────────────────────

  const currentStep = state.loadedProfile?.structure.steps[state.currentStepIndex] ?? null;
  const currentInstructions = currentStep
    ? state.loadedProfile?.instructions[currentStep.label] ?? null
    : state.loadedProfile?.instructions["_header"] ?? null;

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="app-container">

      {/* ── Top bar ── */}
      <header className="app-header">
        <h1>Import Tool</h1>
        {state.loadedProfile && (
          <span className="profile-badge">
            {state.loadedProfile.structure.name} v{state.loadedProfile.structure.version}
          </span>
        )}
      </header>

      {/* ── Global error banner ── */}
      {error && (
        <div className="error-banner">
          <span>{error}</span>
          <button onClick={() => setError(null)}>✕</button>
        </div>
      )}

      {/* ── Main layout ── */}
      <div className="main-layout">

        {/* ── Left sidebar: profile picker + step list ── */}
        <aside className="sidebar">
          <ProfilePicker
            profiles={state.profiles}
            selected={state.selectedProfile}
            onSelect={handleProfileSelect}
            loading={loading}
          />

          {/* Step list — only shown once a profile is loaded */}
          {state.loadedProfile && (
            <nav className="step-list">
              {state.loadedProfile.structure.steps.map((step, i) => {
                const status = state.stepStatuses[step.label] ?? "pending";
                return (
                  <div
                    key={step.label}
                    className={`step-item step-item--${status}`}
                  >
                    <span className="step-number">{i + 1}</span>
                    <span className="step-label">
                      {/* Use the ## heading from instructions if available,
                          otherwise fall back to the label from YAML */}
                      {getStepDisplayName(step.label, state.loadedProfile!.instructions)}
                    </span>
                    <span className="step-status-icon">
                      {status === "complete" && "✓"}
                      {status === "error" && "✕"}
                      {status === "active" && "›"}
                    </span>
                  </div>
                );
              })}
            </nav>
          )}
        </aside>

        {/* ── Center: active step UI ── */}
        <main className="step-area">
          {!state.loadedProfile && (
            <div className="empty-state">
              <p>Select a profile to get started.</p>
            </div>
          )}

          {state.loadedProfile && currentStep && (
            <StepPanel
              step={currentStep}
              stepIndex={state.currentStepIndex}
              totalSteps={state.loadedProfile.structure.steps.length}
              profile={state.loadedProfile}
              attachedFiles={state.attachedFiles}
              onFileAttach={handleFileAttach}
              onStepComplete={handleStepComplete}
              onStepError={handleStepError}
              onOutputReady={handleOutputReady}
            />
          )}

          {/* All steps complete */}
          {state.loadedProfile &&
            state.currentStepIndex >= state.loadedProfile.structure.steps.length && (
            <div className="complete-state">
              <h2>All steps complete</h2>
              <p>Your import files are ready.</p>
              <button onClick={() => handleProfileSelect(state.selectedProfile!)}>
                Start over
              </button>
            </div>
          )}
        </main>

        {/* ── Right panel: instructions for current step ── */}
        <aside className="instructions-panel">
          {currentInstructions && (
            <InstructionsPanel
              markdown={currentInstructions}
              tempDir={state.loadedProfile?.temp_dir ?? ""}
            />
          )}
        </aside>

      </div>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

// Extracts the ## heading text from a step's markdown section to use
// as the display name in the step list, falling back to the raw label.
function getStepDisplayName(
  label: string,
  instructions: Record<string, string>
): string {
  const content = instructions[label];
  if (!content) return label;
  const headingMatch = content.match(/^##\s+(.+)$/m);
  return headingMatch ? headingMatch[1].trim() : label;
}