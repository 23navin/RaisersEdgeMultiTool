// MainPanel.tsx
//
// Scrollable content area. Renders the profile header followed by one
// section per step from the loaded profile, in document order.

import { CheckIcon } from "lucide-react";
import type { LoadedProfile, Step, StepInputRef } from "../types";
import type { FileStatus, GenerateStatus } from "../App";
import { StepSelectFiles } from "./steps/StepSelectFiles";
import { StepGenerateFile } from "./steps/StepGenerateFile";
import { StepImport } from "./steps/StepImport";

type MainPanelProps = {
  loadedProfile: LoadedProfile | null;
  stepsDone: Record<string, boolean>;
  fileName: string | null;
  fileStatus: FileStatus;
  generateStatus: GenerateStatus;
  generateProgress: number;
  onFileSelect: (path: string, name: string) => void;
  onValidate: () => void;
  onGenerate: () => void;
  onDownload: () => void;
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
    <div className="flex items-center justify-between mb-[7px]">
      <h2 className="text-[14px] font-medium text-neutral-900">{name}</h2>
      {done && <CheckIcon size={14} className="text-green-500" />}
    </div>
  );
}

export function MainPanel({
  loadedProfile,
  stepsDone,
  fileName,
  fileStatus,
  generateStatus,
  generateProgress,
  onFileSelect,
  onValidate,
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
        {structure.steps.map((step) => (
          <StepSection
            key={step.label}
            step={step}
            done={stepsDone[step.label] ?? false}
            structure={structure}
            instructions={instructions}
            tempDir={temp_dir}
            fileName={fileName}
            fileStatus={fileStatus}
            generateStatus={generateStatus}
            generateProgress={generateProgress}
            onFileSelect={onFileSelect}
            onValidate={onValidate}
            onGenerate={onGenerate}
            onDownload={onDownload}
          />
        ))}
      </div>
    </main>
  );
}

// ── Step dispatcher ───────────────────────────────────────────────────────────

type StepSectionProps = Omit<MainPanelProps, "loadedProfile" | "stepsDone"> & {
  step: Step;
  done: boolean;
  structure: LoadedProfile["structure"];
  instructions: Record<string, string>;
  tempDir: string;
};

function StepSection({
  step,
  done,
  structure,
  instructions,
  tempDir,
  fileName,
  fileStatus,
  generateStatus,
  generateProgress,
  onFileSelect,
  onValidate,
  onGenerate,
  onDownload,
}: StepSectionProps) {
  const name = stepDisplayName(step.label, instructions);
  const heading = <StepHeading name={name} done={done} />;

  switch (step.type) {
    case "file_input": {
      const inputLabel = refLabel(step.input?.[0] ?? "");
      const inputDef = structure.inputs.find((i) => i.label === inputLabel);
      const accepts = inputDef ? [inputDef.type] : ["csv"];
      return (
        <section>
          {heading}
          <StepSelectFiles
            description={stepBody(instructions[step.label])}
            inputLabel={inputLabel}
            accepts={accepts}
            fileName={fileName}
            fileStatus={fileStatus}
            onFileSelect={onFileSelect}
            onValidate={onValidate}
          />
        </section>
      );
    }
    case "sql_transform": {
      const inputs = (step.input ?? []).map(refLabel);
      const outputs = step.output ?? [];
      return (
        <section>
          {heading}
          <StepGenerateFile
            inputs={inputs}
            outputs={outputs}
            canGenerate={fileStatus === "valid"}
            generateStatus={generateStatus}
            generateProgress={generateProgress}
            onGenerate={onGenerate}
            onDownload={onDownload}
          />
        </section>
      );
    }
    case "manual_instruction": {
      return (
        <section>
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
