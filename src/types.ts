// types.ts
//
// TypeScript types that mirror the Rust structs in profile.rs exactly.
// If you change a struct in Rust, update the matching type here.
// These are used throughout the frontend — import from here, not inline.

// ── Profile listing ───────────────────────────────────────────────────────────
// Returned by list_profiles — lightweight, just enough for the dropdown

export type ProfileSummary = {
  id: string;
  name: string;
  version: string;
  zip_path: string;
};

// ── Structure.yaml types ──────────────────────────────────────────────────────
// Mirror the YAML schema exactly

export type ColumnValidation = {
  label: string;
  required: boolean;
  col_type: string;        // "string" | "number"
  digits?: number;         // only for number columns
  value?: string[];        // allowed values if restricted e.g. ["Alpha", "Beta", "Gamma"]
};

export type InputDefinition = {
  label: string;
  type: string;            // "csv" | "xlsx"
  required: boolean;
  validation?: ColumnValidation[];
};

export type OutputDefinition = {
  label: string;
  type: string;            // "csv"
};

// Steps can reference inputs in two ways:
// Simple:   "Classification"
// Detailed: { label: "Classification", validate: true }
export type StepInputRef =
  | string
  | { label: string; validate?: boolean };

export type Step = {
  label: string;
  type: string;            // "file_input" | "validation" | "sql_transform" | "manual_instruction"
  input?: StepInputRef[];
  sql?: string;            // filename inside sql/ folder
  output?: string[];
};

export type ProfileStructure = {
  id: string;
  name: string;
  version: string;
  min_app_version: string;
  inputs: InputDefinition[];
  outputs: OutputDefinition[];
  steps: Step[];
};

// ── Loaded profile ────────────────────────────────────────────────────────────
// Returned by load_profile — full parse including instructions and SQL

export type LoadedProfile = {
  structure: ProfileStructure;
  instructions: Record<string, string>; // step label → markdown content
  sql_files: Record<string, string>;    // filename → SQL content
  temp_dir: string;
};

// ── App state types ───────────────────────────────────────────────────────────
// Not from Rust — used internally by the frontend to track workflow state

export type StepStatus =
  | "pending"      // not reached yet
  | "active"       // current step
  | "complete"     // finished successfully
  | "error";       // something went wrong

export type FileAttachment = {
  inputLabel: string;    // matches InputDefinition.label e.g. "Classification"
  filePath: string;      // absolute path on disk
  validated: boolean;
  validationErrors: string[];
};

export type AppState = {
  profiles: ProfileSummary[];
  selectedProfile: ProfileSummary | null;
  loadedProfile: LoadedProfile | null;
  currentStepIndex: number;
  stepStatuses: Record<string, StepStatus>;  // step label → status
  attachedFiles: Record<string, FileAttachment>; // inputLabel → attachment
  outputPaths: Record<string, string>;       // outputLabel → file path
};