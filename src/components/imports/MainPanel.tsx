// MainPanel.tsx
//
// Scrollable content area. Renders the profile header followed by one
// section per step from the loaded profile, in document order.

import { CheckIcon } from "lucide-react";
import type { LoadedProfile, Step } from "../../types";
import type {
  FileEntry,
  GenEntry,
  SyncEntry,
  QueryEntry,
  VizEntry,
  FormEntry,
} from "../../App";
import { refLabel, stepTransforms } from "../../lib/profile-utils";
import { StepSelectFiles } from "./steps/StepSelectFiles";
import { StepGenerateFile } from "./steps/StepGenerateFile";
import { StepImport } from "./steps/StepImport";
import { StepCodeTableSync } from "./steps/StepCodeTableSync";
import { StepQuery } from "./steps/StepQuery";
import { StepVisualize } from "./steps/StepVisualize";
import { StepUserInput } from "./steps/StepUserInput";

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
  syncs: Record<string, SyncEntry>;
  onCodeTableSync: (stepLabel: string) => void;
  queries: Record<string, QueryEntry>;
  onRunQuery: (stepLabel: string) => void;
  visualizations: Record<string, VizEntry>;
  onRunVisualization: (stepLabel: string) => void;
  forms: Record<string, FormEntry>;
  onRunUserInput: (stepLabel: string) => void;
  onUserInputChange: (
    stepLabel: string,
    key: string,
    fieldId: string,
    value: string,
  ) => void;
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
  syncs,
  onCodeTableSync,
  queries,
  onRunQuery,
  visualizations,
  onRunVisualization,
  forms,
  onRunUserInput,
  onUserInputChange,
}: MainPanelProps) {
  if (!loadedProfile) {
    return (
      <main className="flex-1 overflow-y-auto px-[22px] py-[18px] text-[13px] text-neutral-500">
        Select a profile to get started.
      </main>
    );
  }

  const { structure, instructions, asset_base, session_id } = loadedProfile;
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
              assetBase={asset_base}
              sessionId={session_id}
              files={files}
              generations={generations}
              onFileSelect={onFileSelect}
              onValidate={onValidate}
              onClearFile={onClearFile}
              onGenerate={onGenerate}
              onDownload={onDownload}
              syncs={syncs}
              onCodeTableSync={onCodeTableSync}
              queries={queries}
              onRunQuery={onRunQuery}
              visualizations={visualizations}
              onRunVisualization={onRunVisualization}
              forms={forms}
              onRunUserInput={onRunUserInput}
              onUserInputChange={onUserInputChange}
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
  assetBase: string;
  sessionId: string;
};

function StepSection({
  step,
  stepNumber,
  done,
  structure,
  instructions,
  assetBase,
  sessionId,
  files,
  generations,
  onFileSelect,
  onValidate,
  onClearFile,
  onGenerate,
  onDownload,
  syncs,
  onCodeTableSync,
  queries,
  onRunQuery,
  visualizations,
  onRunVisualization,
  forms,
  onRunUserInput,
  onUserInputChange,
}: StepSectionProps) {
  const name = `${stepNumber}. ${stepDisplayName(step.label, instructions)}`;
  const heading = <StepHeading name={name} done={done} />;

  switch (step.type) {
    case "file_input": {
      const rows = (step.input ?? []).map((ref) => {
        const lbl = refLabel(ref);
        const entry = files[lbl];
        const def = structure.inputs.find((i) => i.label === lbl);
        return {
          inputLabel: lbl,
          inputType: def?.type ?? "csv",
          fileName: entry?.name ?? null,
          fileStatus: entry?.status ?? ("none" as const),
          errors: entry?.errors ?? [],
          notices: entry?.notices ?? [],
        };
      });
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepSelectFiles
            description={stepBody(instructions[step.label])}
            rows={rows}
            sessionId={sessionId}
            onFileSelect={onFileSelect}
            onValidate={onValidate}
            onClear={onClearFile}
          />
        </section>
      );
    }
    case "sql_transform": {
      const transformRows = stepTransforms(step).map((t, idx) => {
        const inputRefs = t.input ?? [];
        const gen = generations[`${step.label}::${idx}`];
        const inputs = inputRefs.map((r) => {
          const lbl = refLabel(r);
          const f = files[lbl];
          return { label: lbl, ready: f?.status === "valid", kind: "file" as const };
        });
        // Query results this transform declared via query_input show as inputs
        // too — they feed the SQL exactly like a file does. Ready means the
        // re_query step that produces the label has been run.
        const queryInputs = (t.query_input ?? []).map((label) => {
          const producer = structure.steps.find(
            (s) => s.type === "re_query" && s.query_output === label,
          );
          return {
            label,
            ready: producer
              ? queries[producer.label]?.status === "done"
              : false,
            kind: "query" as const,
          };
        });
        // Same for code_table_sync outcomes declared via sync_input. Ready
        // means the sync step ran — including a run that pushed zero rows,
        // which is a legitimate "nothing was missing" result.
        const syncInputs = (t.sync_input ?? []).map((label) => {
          const producer = structure.steps.find(
            (s) => s.type === "code_table_sync" && s.sync_output === label,
          );
          return {
            label,
            ready: producer ? syncs[producer.label]?.status === "done" : false,
            kind: "sync" as const,
          };
        });
        // Same for the values a user_input step published. Ready means every
        // required box on every row is filled — a half-filled form publishes
        // nulls, and joining to those would empty a column of the output.
        const formInputs = (t.form_input ?? []).map((label) => {
          const producer = structure.steps.find(
            (s) => s.type === "user_input" && s.form_output === label,
          );
          const state = producer ? forms[producer.label] : undefined;
          return {
            label,
            ready: state?.status === "done" && (state.result?.complete ?? false),
            kind: "form" as const,
          };
        });
        const outputs = (t.output ?? []).map((label) => ({
          label,
          ready: gen?.status === "done",
        }));
        const filesReady = inputRefs.every((r) => {
          const lbl = refLabel(r);
          const def = structure.inputs.find((i) => i.label === lbl);
          const f = files[lbl];
          if (def?.required) return f?.status === "valid";
          return !f || f.status === "valid";
        });
        // Query results are hard prerequisites, not optional extras: the SQL
        // substitutes {{query:Label}} with the file the producing re_query step
        // wrote, so generating before that step has run can only fail.
        const canGenerate =
          filesReady &&
          queryInputs.every((q) => q.ready) &&
          formInputs.every((f) => f.ready);
        return {
          inputs: [...inputs, ...queryInputs, ...syncInputs, ...formInputs],
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
    case "re_query": {
      const state = queries[step.label];
      const inputRefs = step.input ?? [];
      const inputs = inputRefs.map((r) => {
        const lbl = refLabel(r);
        return { label: lbl, ready: files[lbl]?.status === "valid" };
      });
      // Same readiness rule as a transform: every required input valid.
      const canRun = inputRefs.every((r) => {
        const lbl = refLabel(r);
        const def = structure.inputs.find((i) => i.label === lbl);
        const f = files[lbl];
        if (def?.required) return f?.status === "valid";
        return !f || f.status === "valid";
      });
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepQuery
            description={stepBody(instructions[step.label])}
            query={{
              queryOutput: step.query_output ?? "(unnamed)",
              inputs,
              canRun,
              status: state?.status ?? "idle",
              result: state?.result,
              error: state?.error,
              onRun: () => onRunQuery(step.label),
            }}
          />
        </section>
      );
    }
    case "code_table_sync": {
      const state = syncs[step.label];
      const inputRefs = step.input ?? [];
      const inputs = inputRefs.map((r) => {
        const lbl = refLabel(r);
        return { label: lbl, ready: files[lbl]?.status === "valid" };
      });
      // Same readiness rule as a transform: every required input valid.
      const canRun = inputRefs.every((r) => {
        const lbl = refLabel(r);
        const def = structure.inputs.find((i) => i.label === lbl);
        const f = files[lbl];
        if (def?.required) return f?.status === "valid";
        return !f || f.status === "valid";
      });
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepCodeTableSync
            description={stepBody(instructions[step.label])}
            sync={{
              operation: step.operation ?? "create",
              codeTable: step.code_table ?? step.code_table_id ?? "(unspecified)",
              inputs,
              canRun,
              status: state?.status ?? "idle",
              result: state?.result,
              error: state?.error,
              onRun: () => onCodeTableSync(step.label),
            }}
          />
        </section>
      );
    }
    case "visualization": {
      const state = visualizations[step.label];
      // Every source the step reads, in the order it declares them: uploaded
      // files first, then the named results earlier steps published. `ready`
      // drives both the checkmarks and whether the button is live.
      const fileSources = (step.input ?? []).map((r) => {
        const lbl = refLabel(r);
        return { label: lbl, ready: files[lbl]?.status === "valid", kind: "file" as const };
      });
      const querySources = (step.query_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "re_query" && s.query_output === label,
        );
        return {
          label,
          ready: producer ? queries[producer.label]?.status === "done" : false,
          kind: "query" as const,
        };
      });
      const syncSources = (step.sync_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "code_table_sync" && s.sync_output === label,
        );
        return {
          label,
          ready: producer ? syncs[producer.label]?.status === "done" : false,
          kind: "sync" as const,
        };
      });
      const formSources = (step.form_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "user_input" && s.form_output === label,
        );
        const state = producer ? forms[producer.label] : undefined;
        return {
          label,
          ready: state?.status === "done" && (state.result?.complete ?? false),
          kind: "form" as const,
        };
      });
      const sources = [...fileSources, ...querySources, ...syncSources, ...formSources];
      // Same readiness rule as a transform for files (optional inputs may be
      // absent); an upstream result is only usable once its step has run.
      const canRun =
        (step.input ?? []).every((r) => {
          const lbl = refLabel(r);
          const def = structure.inputs.find((i) => i.label === lbl);
          const f = files[lbl];
          if (def?.required) return f?.status === "valid";
          return !f || f.status === "valid";
        }) && [...querySources, ...syncSources, ...formSources].every((s) => s.ready);
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepVisualize
            description={stepBody(instructions[step.label])}
            viz={{
              spec: step.visualization ?? { type: "table" },
              sources,
              canRun,
              status: state?.status ?? "idle",
              data: state?.data,
              error: state?.error,
              onRun: () => onRunVisualization(step.label),
            }}
          />
        </section>
      );
    }
    case "user_input": {
      const state = forms[step.label];
      // The step's rows_sql reads the same kinds of source a visualization
      // does: uploaded files, plus any upstream result the step declares.
      const fileSources = (step.input ?? []).map((r) => {
        const lbl = refLabel(r);
        return { label: lbl, ready: files[lbl]?.status === "valid", kind: "file" as const };
      });
      const querySources = (step.query_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "re_query" && s.query_output === label,
        );
        return {
          label,
          ready: producer ? queries[producer.label]?.status === "done" : false,
          kind: "query" as const,
        };
      });
      const syncSources = (step.sync_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "code_table_sync" && s.sync_output === label,
        );
        return {
          label,
          ready: producer ? syncs[producer.label]?.status === "done" : false,
          kind: "sync" as const,
        };
      });
      // A form whose rows come out of an earlier form's answers. Ready means
      // that form is complete — half-answered rows would derive the wrong
      // question set here.
      const formSources = (step.form_input ?? []).map((label) => {
        const producer = structure.steps.find(
          (s) => s.type === "user_input" && s.form_output === label,
        );
        const st = producer ? forms[producer.label] : undefined;
        return {
          label,
          ready: st?.status === "done" && (st.result?.complete ?? false),
          kind: "form" as const,
        };
      });
      const canRun =
        (step.input ?? []).every((r) => {
          const lbl = refLabel(r);
          const def = structure.inputs.find((i) => i.label === lbl);
          const f = files[lbl];
          if (def?.required) return f?.status === "valid";
          return !f || f.status === "valid";
        }) && [...querySources, ...syncSources, ...formSources].every((s) => s.ready);
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepUserInput
            description={stepBody(instructions[step.label])}
            form={{
              formOutput: step.form_output ?? "(unnamed)",
              fields: step.fields ?? [],
              keyed: Boolean(step.rows_sql),
              sources: [...fileSources, ...querySources, ...syncSources, ...formSources],
              canRun,
              status: state?.status ?? "idle",
              result: state?.result,
              values: state?.values ?? {},
              error: state?.error,
              onRun: () => onRunUserInput(step.label),
              onChange: (key, fieldId, value) =>
                onUserInputChange(step.label, key, fieldId, value),
            }}
          />
        </section>
      );
    }
    case "manual_instruction": {
      return (
        <section id={`step-${step.label}`} className="scroll-mt-[18px]">
          {heading}
          <StepImport
            markdown={stepBody(instructions[step.label])}
            assetBase={assetBase}
          />
        </section>
      );
    }
    default:
      return null;
  }
}
