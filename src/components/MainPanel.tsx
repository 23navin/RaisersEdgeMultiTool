// MainPanel.tsx
//
// Scrollable content area. Renders the profile header followed by one
// section per step from the loaded profile, in document order.

import { CheckIcon } from "lucide-react";
import type { LoadedProfile, Step, StepInputRef } from "../types";
import type { FileEntry, GenEntry } from "../App";
import { StepSelectFiles } from "./steps/StepSelectFiles";
import { StepGenerateFile } from "./steps/StepGenerateFile";
import { StepImport } from "./steps/StepImport";

type MainPanelProps = {
  loadedProfile: LoadedProfile | null;
  stepsDone: Record<string, boolean>;
  files: Record<string, FileEntry>;
  generations: Record<string, GenEntry>;
  onFileSelect: (inputLabel: string, path: string, name: string) => void;
  onValidate: (inputLabel: string) => void;
  onClearFile: (inputLabel: string) => void;
  onGenerate: (stepLabel: string, transformIdx: number) => void;
  onDownload: (stepLabel: string, transformIdx: number, outputLabel: string) => void;
};

// ── Helpers ───────────────────────────────────────────────────────────────────

// Extracts the # heading body as profile description: everything after the
// first heading line, trimmed. Falls back to empty.
function profileDescription(headerMd: string | undefined): string {
  if (!headerMd) return "";
  return headerMd
    .replace(/^#\s+.+$/m, "")
    .replace(/^---\s*$/m, "")
    .trim();
}

// Extracts the inline body of a step's markdown: drops the ## heading line.
function stepBody(md: string | undefined): string {
  if (!md) return "";
  return md.replace(/^##\s+.+$/m, "").trim();
}

function stepDisplayName(label: string, instructions: Record<string, string>) {
  const md = instructions[label];
  if (!md) return label;
  const m = md.match(/^##\s+(.+)$/m);
  return m ? m[1].trim() : label;
}

// Normalizes a step.input entry (string or { label, validate? }) to a label.
function refLabel(ref: StepInputRef): string {
  return typeof ref === "string" ? ref : ref.label;
}

// Heading row used by every step section.
function StepHeading({ name, done }: { name: string; done: boolean }) {
  return (
    <div className="flex items-center gap-[6px] mb-[7px]">
      <h2 className="text-[14px] font-medium text-neutral-900">{name}</h2>
      {done && <CheckIcon size={14} className="text-green-500" />}
    </div>
  );
}

export function MainPanel({
  loadedProfile,
  stepsDone,
  files,
  generations,
  onFileSelect,
  onValidate,
  onClearFile,
  onGenerate,
  onDownload,
}: MainPanelProps) {
  if (!loadedProfile) {
    return (
      <main className="flex-1 overflow-y-auto px-[22px] py-[18px] text-[13px] text-neutral-500">
        Select a profile to get started.
      </main>
    );
  }

  const { structure, instructions, temp_dir } = loadedProfile;
  const description = profileDescription(instructions["_header"]);

  return (
    <main className="flex-1 overflow-y-auto px-[22px] py-[18px]">
      <div
        key={structure.id}
        className="animate-in fade-in duration-200"
      >
        {/* Profile header */}
        <div className="pb-[14px] border-b border-neutral-200 mb-[20px]">
          <h1 className="text-[17px] font-medium text-neutral-900">
            {structure.name}
          </h1>
          {description && (
            <p className="text-[13px] text-neutral-500 leading-relaxed mt-[3px]">
              {description}
            </p>
          )}
        </div>

        {/* Steps */}
        <div className="flex flex-col gap-[20px]">
          {structure.steps.map((step, idx) => (
            <StepSection
              key={step.label}
              step={step}
              stepNumber={idx + 1}
              done={stepsDone[step.label] ?? false}
              structure={structure}
              instructions={instructions}
              tempDir={temp_dir}
              files={files}
              generations={generations}
              onFileSelect={onFileSelect}
              onValidate={onValidate}
              onClearFile={onClearFile}
              onGenerate={onGenerate}
              onDownload={onDownload}
            />
          ))}
        </div>
      </div>
    </main>
  );
}

// ── Step dispatcher ───────────────────────────────────────────────────────────

type StepSectionProps = Omit<MainPanelProps, "loadedProfile" | "stepsDone"> & {
  step: Step;
  stepNumber: number;
  done: boolean;
  structure: LoadedProfile["structure"];
  instructions: Record<string, string>;
  tempDir: string;
};

function StepSection({
  step,
  stepNumber,
  done,
  structure,
  instructions,
  tempDir,
  files,
  generations,
  onFileSelect,
  onValidate,
  onClearFile,
  onGenerate,
  onDownload,
}: StepSectionProps) {
  const name = `${stepNumber}. ${stepDisplayName(step.label, instructions)}`;
  const heading = <StepHeading name={name} done={done} />;

  switch (step.type) {
    case "file_input": {
      const rows = (step.input ?? []).map((ref) => {
        const lbl = refLabel(ref);
        const entry = files[lbl];
        return {
          inputLabel: lbl,
          fileName: entry?.name ?? null,
          fileStatus: entry?.status ?? ("none" as const),
          errors: entry?.errors ?? [],
        };
      });
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepSelectFiles
            description={stepBody(instructions[step.label])}
            rows={rows}
            onFileSelect={onFileSelect}
            onValidate={onValidate}
            onClear={onClearFile}
          />
        </section>
      );
    }
    case "sql_transform": {
      const transforms =
        step.transforms && step.transforms.length > 0
          ? step.transforms
          : [{ input: step.input, sql: step.sql ?? "", output: step.output }];

      const transformRows = transforms.map((t, idx) => {
        const inputRefs = t.input ?? [];
        const gen = generations[`${step.label}::${idx}`];
        const inputs = inputRefs.map((r) => {
          const lbl = refLabel(r);
          const f = files[lbl];
          return { label: lbl, ready: f?.status === "valid" };
        });
        const outputs = (t.output ?? []).map((label) => ({
          label,
          ready: gen?.status === "done",
        }));
        const canGenerate = inputRefs.every((r) => {
          const lbl = refLabel(r);
          const def = structure.inputs.find((i) => i.label === lbl);
          const f = files[lbl];
          if (def?.required) return f?.status === "valid";
          return !f || f.status === "valid";
        });
        return {
          inputs,
          outputs,
          canGenerate,
          generateStatus: gen?.status ?? ("idle" as const),
          generateProgress: gen?.progress ?? 0,
          errors: gen?.errors ?? [],
          notices: gen?.notices ?? [],
          onGenerate: () => onGenerate(step.label, idx),
          onDownload: (outputLabel: string) =>
            onDownload(step.label, idx, outputLabel),
        };
      });
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepGenerateFile transforms={transformRows} />
        </section>
      );
    }
    case "manual_instruction": {
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepImport
            markdown={stepBody(instructions[step.label])}
            tempDir={tempDir}
          />
        </section>
      );
    }
    default:
      return null;
  }
}
