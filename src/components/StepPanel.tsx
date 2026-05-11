// components/StepPanel.tsx
//
// Renders the active step's UI. Step type determines what gets shown:
//
//   file_input     → file picker per input, attach button
//   validation     → triggers validate_file command, shows results
//   sql_transform  → triggers run_profile command, shows output path
//   manual_instruction → just a confirm/continue button
//
// This is the component that calls invoke() for file operations.
// It receives callbacks to report results back up to App.tsx.

import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  Step,
  LoadedProfile,
  FileAttachment,
  StepInputRef,
} from "../types";

type Props = {
  step: Step;
  stepIndex: number;
  totalSteps: number;
  profile: LoadedProfile;
  attachedFiles: Record<string, FileAttachment>;
  outputPaths: Record<string, string>;
  onFileAttach: (inputLabel: string, filePath: string) => void;
  onStepComplete: (stepLabel: string) => void;
  onStepError: (stepLabel: string, message: string) => void;
  onOutputReady: (outputLabel: string, filePath: string) => void;
};

// ── Helpers ───────────────────────────────────────────────────────────────────

// Normalizes StepInputRef to just the label string
function getInputLabel(ref: StepInputRef): string {
  return typeof ref === "string" ? ref : ref.label;
}

// Finds the InputDefinition for a given label from the profile structure
function getInputDef(profile: LoadedProfile, label: string) {
  return profile.structure.inputs.find((i) => i.label === label);
}

// ── StepPanel ─────────────────────────────────────────────────────────────────

export function StepPanel({
  step,
  stepIndex,
  totalSteps,
  profile,
  attachedFiles,
  outputPaths,
  onFileAttach,
  onStepComplete,
  onStepError,
  onOutputReady,
}: Props) {
  const [running, setRunning] = useState(false);
  const [validationResults, setValidationResults] = useState<
    Record<string, { ok: boolean; errors: string[] }>
  >({});

  // ── file_input step ─────────────────────────────────────────────────────────
  // Shows a file picker for each input the step requires.
  // Uses Tauri's dialog plugin to open a native file picker.

  async function handleFilePick(inputLabel: string) {
    const inputDef = getInputDef(profile, inputLabel);
    const extensions = inputDef?.input_type === "csv"
      ? ["csv"]
      : inputDef?.input_type === "xlsx"
      ? ["xlsx", "xls"]
      : ["csv", "xlsx"];

    const selected = await open({
      multiple: false,
      filters: [{ name: "Data files", extensions }],
    });

    if (selected && typeof selected === "string") {
      onFileAttach(inputLabel, selected);
    }
  }

  // Check all required inputs for this step have files attached
  function allFilesAttached(): boolean {
    if (!step.input) return true;
    return step.input.every((ref) => {
      const label = getInputLabel(ref);
      const inputDef = getInputDef(profile, label);
      if (!inputDef?.required) return true;
      return !!attachedFiles[label];
    });
  }

  // ── validation step ─────────────────────────────────────────────────────────
  // Calls validate_file for each attached file that needs validation.
  // Reports column-level errors back to the user before running SQL.

  async function handleValidation() {
    if (!step.input) return;
    setRunning(true);
    const results: Record<string, { ok: boolean; errors: string[] }> = {};

    for (const ref of step.input) {
      const label = getInputLabel(ref);
      const needsValidation = typeof ref === "object" && ref.validate === true;
      const file = attachedFiles[label];

      if (!file) {
        results[label] = { ok: false, errors: ["No file attached"] };
        continue;
      }

      if (!needsValidation) {
        results[label] = { ok: true, errors: [] };
        continue;
      }

      try {
        // validate_file checks extension + expected columns from profile
        await invoke("validate_file", {
          filePath: file.filePath,
          profileId: profile.structure.id,
          inputLabel: label,
          zipPath: profile.temp_dir, // backend re-reads validation rules
        });
        results[label] = { ok: true, errors: [] };
      } catch (e) {
        results[label] = { ok: false, errors: [e as string] };
      }
    }

    setValidationResults(results);
    setRunning(false);

    const allPassed = Object.values(results).every((r) => r.ok);
    if (allPassed) {
      onStepComplete(step.label);
    } else {
      onStepError(step.label, "Validation failed — check errors above.");
    }
  }

  // ── sql_transform step ──────────────────────────────────────────────────────
  // Calls run_profile which executes the SQL and writes the output CSV.
  // The output file path comes back from Rust and is stored in App.tsx.

  async function handleTransform() {
    if (!step.sql || !step.input || !step.output) return;
    setRunning(true);

    const inputLabel = getInputLabel(step.input[0]);
    const file = attachedFiles[inputLabel];
    if (!file) {
      onStepError(step.label, `No file attached for ${inputLabel}`);
      setRunning(false);
      return;
    }

    try {
      const result = await invoke<{ output_path: string; row_count: number }>(
        "run_profile",
        {
          filePath: file.filePath,
          sqlFile: step.sql,
          zipPath: profile.temp_dir,
          outputLabel: step.output[0],
        }
      );

      onOutputReady(step.output[0], result.output_path);
      onStepComplete(step.label);
    } catch (e) {
      onStepError(step.label, `Transform failed: ${e}`);
    } finally {
      setRunning(false);
    }
  }

  // ── manual_instruction step ─────────────────────────────────────────────────
  // No app action — just instructions and a continue button.

  function handleManualContinue() {
    onStepComplete(step.label);
  }

  // ── Render ──────────────────────────────────────────────────────────────────

  return (
    <div className="step-panel">
      <div className="step-panel__header">
        <span className="step-counter">Step {stepIndex + 1} of {totalSteps}</span>
        <span className="step-type-badge">{step.type}</span>
      </div>

      {/* ── file_input ── */}
      {step.type === "file_input" && (
        <div className="step-panel__body">
          <p>Attach the required files below.</p>
          {step.input?.map((ref) => {
            const label = getInputLabel(ref);
            const file = attachedFiles[label];
            const inputDef = getInputDef(profile, label);
            return (
              <div key={label} className="file-input-row">
                <span className="file-input-label">
                  {label}
                  {inputDef?.required && <span className="required-mark"> *</span>}
                </span>
                <span className="file-input-path">
                  {file ? file.filePath.split("/").pop() : "No file selected"}
                </span>
                <button onClick={() => handleFilePick(label)}>
                  {file ? "Change" : "Browse"}
                </button>
              </div>
            );
          })}
          <div className="step-panel__actions">
            <button
              disabled={!allFilesAttached()}
              onClick={() => onStepComplete(step.label)}
            >
              Continue
            </button>
          </div>
        </div>
      )}

      {/* ── validation ── */}
      {step.type === "validation" && (
        <div className="step-panel__body">
          <p>Validate attached files against the profile rules.</p>
          {Object.entries(validationResults).map(([label, result]) => (
            <div
              key={label}
              className={`validation-result ${result.ok ? "validation-result--ok" : "validation-result--error"}`}
            >
              <span>{label}</span>
              <span>{result.ok ? "✓ Passed" : "✕ Failed"}</span>
              {result.errors.map((e, i) => (
                <p key={i} className="validation-error-detail">{e}</p>
              ))}
            </div>
          ))}
          <div className="step-panel__actions">
            <button onClick={handleValidation} disabled={running}>
              {running ? "Validating..." : "Run Validation"}
            </button>
          </div>
        </div>
      )}

      {/* ── sql_transform ── */}
      {step.type === "sql_transform" && (
        <div className="step-panel__body">
          <p>Generate the import file using the profile transform.</p>
          {step.output?.map((outputLabel) => {
            const path = outputPaths[outputLabel];
            return path ? (
              <div key={outputLabel} className="output-result">
                <span>✓ {outputLabel}</span>
                <span className="output-path">{path}</span>
              </div>
            ) : null;
          })}
          <div className="step-panel__actions">
            <button onClick={handleTransform} disabled={running}>
              {running ? "Generating..." : "Generate Import File"}
            </button>
          </div>
        </div>
      )}

      {/* ── manual_instruction ── */}
      {step.type === "manual_instruction" && (
        <div className="step-panel__body">
          <p>Complete the steps shown in the instructions panel, then continue.</p>
          <div className="step-panel__actions">
            <button onClick={handleManualContinue}>Mark Complete</button>
          </div>
        </div>
      )}
    </div>
  );
}