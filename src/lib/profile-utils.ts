import type { Step, StepInputRef, SqlTransform } from "../types";

// Normalizes a step.input entry (string or { label, validate? }) to a label.
export function refLabel(ref: StepInputRef): string {
  return typeof ref === "string" ? ref : ref.label;
}

// Normalizes a sql_transform step to its list of transforms. Supports the
// multi-transform `transforms` array and the single-transform shortcut where
// input/sql/output/notices live directly on the step.
export function stepTransforms(step: Step): SqlTransform[] {
  if (step.transforms && step.transforms.length > 0) return step.transforms;
  return [
    {
      input: step.input,
      sql: step.sql ?? "",
      output: step.output,
      notices: step.notices,
    },
  ];
}
