// App.tsx
//
// Root component. Owns all shared state. Renders the shell:
// Titlebar on top, then a floating panel containing Sidebar + MainPanel.
//
// Phase 1: mock data only. invoke() is wired up in later steps.

import { useState } from "react";
import type {
  LoadedProfile,
  ProfileSummary,
  SqlError,
  StepInputRef,
  ValidationError,
} from "./types";
import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { MainPanel } from "./components/MainPanel";

export type FileStatus = "none" | "pending" | "valid" | "invalid";
export type GenerateStatus = "idle" | "running" | "done" | "error";

export type FileEntry = {
  path: string;
  name: string;
  status: FileStatus;
  errors?: ValidationError[];
};
export type GenEntry = {
  status: GenerateStatus;
  progress: number;
  errors?: SqlError[];
};

function refLabel(ref: StepInputRef): string {
  return typeof ref === "string" ? ref : ref.label;
}

// Composite key for a single transform within a sql_transform step.
function genKey(stepLabel: string, transformIdx: number): string {
  return `${stepLabel}::${transformIdx}`;
}

// ── Mock data (phase 1) ───────────────────────────────────────────────────────
// Mirrors the real shape from types.ts so wiring later is a straight swap.

const MOCK_PROFILES: ProfileSummary[] = [
  { id: "test1", name: "Simple Import", version: "1.0.0", zip_path: "" },
  { id: "test2", name: "Multi-File Vendor Import", version: "2.0.0", zip_path: "" },
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

const MOCK_PROFILE_2: LoadedProfile = {
  structure: {
    id: "test2",
    name: "Multi-File Vendor Import",
    version: "2.0.0",
    min_app_version: "0.1.0",
    inputs: [
      {
        label: "Inventory",
        type: "csv",
        required: true,
        validation: [
          { label: "Item #", required: true, col_type: "number", digits: 6 },
          {
            label: "Category",
            required: true,
            col_type: "string",
            value: ["Alpha", "Beta", "Gamma"],
          },
        ],
      },
      {
        label: "Pricing",
        type: "xlsx",
        required: true,
        validation: [
          { label: "SKU", required: true, col_type: "string" },
          { label: "Price", required: true, col_type: "number" },
        ],
      },
      {
        label: "Categories",
        type: "csv",
        required: false,
      },
    ],
    outputs: [
      { label: "Inventory_Import", type: "csv" },
      { label: "Pricing_Update", type: "csv" },
      { label: "Audit_Log", type: "csv" },
    ],
    steps: [
      // Inventory and Categories come from the same VendorX export, so they
      // share a single upload step with two file rows. Pricing is a separate
      // source, so it's its own step.
      {
        label: "UploadVendorX",
        type: "file_input",
        input: [
          { label: "Inventory", validate: true },
          { label: "Categories", validate: false },
        ],
      },
      {
        label: "UploadPricing",
        type: "file_input",
        input: [{ label: "Pricing", validate: true }],
      },
      // Both transforms run in one step. Each pipeline diagram + Generate row
      // is independent.
      {
        label: "BuildOutputs",
        type: "sql_transform",
        transforms: [
          {
            input: ["Inventory", "Categories"],
            sql: "inventory.sql",
            output: ["Inventory_Import"],
          },
          {
            input: ["Pricing"],
            sql: "pricing.sql",
            output: ["Pricing_Update"],
          },
          {
            input: ["Categories", "Pricing"],
            sql: "audit.sql",
            output: ["Audit_Log"],
          },
        ],
      },
      { label: "ImportFiles", type: "manual_instruction" },
    ],
  },
  instructions: {
    _header:
      "# Multi-File Vendor Import\n\nProcesses `inventory.csv`, `pricing.xlsx`, and `categories.csv` from VendorX into the target database.",
    UploadVendorX:
      "## Upload VendorX Files\n\nFrom the VendorX portal, download `inventory.csv` (required — `Item #` 6 digits, `Category` one of `Alpha`, `Beta`, `Gamma`) and `categories.csv` (optional reference list, skip if unchanged).",
    UploadPricing:
      "## Upload Pricing\n\nProvide the `pricing.xlsx` workbook. Use the `Current` sheet with the header on row 1.",
    BuildOutputs:
      "## Build Output Files\n\nProduces both `Inventory_Import` (merges inventory with category metadata) and `Pricing_Update` (diff against current DB values) in this step.",
    ImportFiles:
      "## Import into database\n\nRun the imports in this order using the `BulkImport` profile:\n\n1. `Inventory_Import` first.\n\n2. Then `Pricing_Update`.\n\n![Import tool](assets/import_profile.png)\n\nIf there are exceptions, contact `vendor@example.com`.",
  },
  sql_files: {},
  temp_dir: "",
};

const MOCK_PROFILE_MAP: Record<string, LoadedProfile> = {
  test1: MOCK_PROFILE,
  test2: MOCK_PROFILE_2,
};

export default function App() {
  // ── State ───────────────────────────────────────────────────────────────────
  const [profiles] = useState<ProfileSummary[]>(MOCK_PROFILES);
  const [selectedProfile, setSelectedProfile] = useState<string | null>("test1");
  const loadedProfile: LoadedProfile | null =
    selectedProfile != null ? MOCK_PROFILE_MAP[selectedProfile] ?? null : null;

  const [files, setFiles] = useState<Record<string, FileEntry>>({});
  const [generations, setGenerations] = useState<Record<string, GenEntry>>({});

  // Normalizes a sql_transform step to a list of (input, sql, output) tuples.
  // Supports both the multi-transform `transforms` array and the legacy
  // single-transform step-level fields.
  const stepTransforms = (step: LoadedProfile["structure"]["steps"][number]) => {
    if (step.transforms && step.transforms.length > 0) return step.transforms;
    return [{ input: step.input, sql: step.sql ?? "", output: step.output }];
  };

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

  // ── Mock handlers (replaced with invoke() in later phase) ──────────────────
  const handleFileSelect = (inputLabel: string, path: string, name: string) => {
    setFiles((prev) => ({
      ...prev,
      [inputLabel]: { path, name, status: "pending" },
    }));
    resetGenerationsConsuming(inputLabel);
  };

  const handleValidate = (inputLabel: string) => {
    // mock: "Inventory" deliberately fails so the error table is demoable.
    // Every other input passes validation.
    const mockErrors: ValidationError[] | undefined =
      inputLabel === "Inventory"
        ? [
            { row: 5, column: "Item #", value: "ABC123", message: "not a number" },
            { row: 12, column: "Category", value: "Delta", message: "not in [Alpha, Beta, Gamma]" },
            { row: 18, column: "Item #", value: "12345", message: "expected 6 digits, got 5" },
          ]
        : undefined;
    setFiles((prev) => {
      const cur = prev[inputLabel];
      if (!cur) return prev;
      return {
        ...prev,
        [inputLabel]: {
          ...cur,
          status: mockErrors ? "invalid" : "valid",
          errors: mockErrors,
        },
      };
    });
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

  const handleGenerate = (stepLabel: string, transformIdx: number) => {
    const key = genKey(stepLabel, transformIdx);

    // Mock: look up the transform's SQL filename. If it's "audit.sql", the
    // run deliberately fails at the end so the error table is demoable.
    const step = loadedProfile?.structure.steps.find((s) => s.label === stepLabel);
    const transforms = step ? stepTransforms(step) : [];
    const willFail = transforms[transformIdx]?.sql === "audit.sql";
    const mockErrors: SqlError[] = [
      { line: 12, errorType: "Binder", message: 'Referenced column "SKU" not found in FROM clause' },
      { line: 24, errorType: "Parser", message: 'syntax error at or near "GROUP"' },
      { errorType: "Runtime", message: "Conversion Error: Could not cast value 'N/A' to DOUBLE" },
    ];

    setGenerations((prev) => ({
      ...prev,
      [key]: { status: "running", progress: 0 },
    }));
    let p = 0;
    const tick = () => {
      p += 20;
      const reachedEnd = p >= 100;
      setGenerations((prev) => ({
        ...prev,
        [key]: reachedEnd
          ? willFail
            ? { status: "error", progress: 100, errors: mockErrors }
            : { status: "done", progress: 100 }
          : { status: "running", progress: p },
      }));
      if (!reachedEnd) setTimeout(tick, 120);
    };
    setTimeout(tick, 120);
  };

  const handleDownload = (_stepLabel: string, _transformIdx: number) => {
    // mock: no-op
  };

  // Resets workflow state for the currently-loaded profile.
  // Keeps profile selection; only clears file + generation state.
  const handleReset = () => {
    setFiles({});
    setGenerations({});
  };

  const handleSelectProfile = (id: string | null) => {
    setSelectedProfile(id);
    handleReset();
  };

  return (
    <div className="flex flex-col h-screen bg-neutral-100 text-neutral-900">
      <Titlebar />
      <div className="flex-1 p-4 pt-0 overflow-hidden">
        <div className="flex h-full bg-white rounded-xl border border-neutral-200 shadow-md overflow-hidden">
          <Sidebar
            profiles={profiles}
            selectedProfile={selectedProfile}
            onSelectProfile={handleSelectProfile}
            loadedProfile={loadedProfile}
            stepsDone={stepsDone}
            onReset={handleReset}
          />
          <MainPanel
            loadedProfile={loadedProfile}
            stepsDone={stepsDone}
            files={files}
            generations={generations}
            onFileSelect={handleFileSelect}
            onValidate={handleValidate}
            onClearFile={handleClearFile}
            onGenerate={handleGenerate}
            onDownload={handleDownload}
          />
        </div>
      </div>
    </div>
  );
}
