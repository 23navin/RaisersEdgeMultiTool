// StepGenerateFile.tsx
//
// Renders steps with type=sql_transform.
// One step's card can contain multiple transforms — each transform is its
// own pipeline diagram (inputs → DB → outputs) plus a generate/progress/
// download row. Multiple transforms are separated by a thin divider.

import {
  DatabaseIcon,
  FileTextIcon,
  TableIcon,
  PlayIcon,
  DownloadIcon,
  CheckIcon,
  XIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "../ui/button";
import type { GenerateStatus } from "../../App";
import type { SqlError } from "../../types";

export type PipeItem = { label: string; ready: boolean };

export type TransformRow = {
  inputs: PipeItem[];
  outputs: PipeItem[];
  canGenerate: boolean;
  generateStatus: GenerateStatus;
  generateProgress: number;
  errors: SqlError[];
  onGenerate: () => void;
  onDownload: () => void;
};

type Props = {
  transforms: TransformRow[];
};

function PipeNode({
  icon: Icon,
  label,
  ready,
}: {
  icon: LucideIcon;
  label: string;
  ready: boolean;
}) {
  const StatusIcon = ready ? CheckIcon : XIcon;
  const statusColor = ready ? "text-green-600" : "text-red-600";
  return (
    <div className="inline-flex items-center gap-[5px] px-[10px] py-[4px] text-neutral-500 text-[11px] whitespace-nowrap">
      <Icon size={12} />
      <span className="overflow-hidden text-ellipsis">{label}</span>
      <StatusIcon size={12} className={statusColor} strokeWidth={2.5} />
    </div>
  );
}

function ArrowSVG() {
  return (
    <div className="flex items-center w-full">
      <div className="flex-1 h-px bg-[#d1d5db]" />
      <svg width="9" height="12" viewBox="0 0 9 12" className="shrink-0">
        <polygon points="0,2 9,6 0,10" fill="#d1d5db" />
      </svg>
    </div>
  );
}

const baseBtn =
  "rounded-none h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none transform-gpu";
const activeBtn =
  "bg-[#1a1a1a] hover:bg-[#2a2a2a] text-white cursor-pointer";
const doneBtn =
  "bg-[#e0ddd8] hover:bg-[#d4d0c9] text-neutral-500 hover:text-neutral-700 disabled:opacity-100 cursor-pointer";
const notReadyBtn =
  "bg-neutral-200 hover:bg-neutral-200 text-neutral-400 disabled:opacity-100 cursor-not-allowed";

function TransformBlock({ t }: { t: TransformRow }) {
  const generateDisabled = !t.canGenerate || t.generateStatus === "running";
  const downloadDisabled = t.generateStatus !== "done";

  const fillColor =
    t.generateStatus === "done"
      ? "bg-green-500"
      : t.generateStatus === "error"
      ? "bg-red-500"
      : "bg-neutral-400";

  // Bar track stays the same grey as a disabled button in every state;
  // the colored fill grows on top of it.
  const trackColor = "bg-neutral-200";

  const generateBtnStyle =
    t.generateStatus === "done"
      ? doneBtn
      : t.canGenerate
      ? activeBtn
      : notReadyBtn;

  const downloadBtnStyle =
    t.generateStatus === "done" ? activeBtn : notReadyBtn;

  return (
    <div>
      {/* Pipeline diagram — two equal-width sides flank the database icon
          so the icon stays at the cell's horizontal center even when the
          input and output pill columns have different widths. */}
      <div className="flex items-center w-full mb-[10px]">
        <div className="flex-1 flex items-center justify-end min-w-0">
          <div className="flex flex-col gap-[5px] shrink-0">
            {t.inputs.map((item) => (
              <PipeNode
                key={item.label}
                icon={FileTextIcon}
                label={item.label}
                ready={item.ready}
              />
            ))}
          </div>
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
        </div>

        <div className="shrink-0 px-[4px]">
          <DatabaseIcon size={22} className="text-neutral-400" />
        </div>

        <div className="flex-1 flex items-center justify-start min-w-0">
          <div className="flex items-center min-w-[20px] max-w-[50px] flex-1 px-[4px]">
            <ArrowSVG />
          </div>
          <div className="flex flex-col gap-[5px] shrink-0">
            {t.outputs.map((item) => (
              <PipeNode
                key={item.label}
                icon={TableIcon}
                label={item.label}
                ready={item.ready}
              />
            ))}
          </div>
        </div>
      </div>

      {/* Generate row */}
      <div className="flex items-center gap-[8px]">
        <Button
          type="button"
          onClick={t.onGenerate}
          disabled={generateDisabled}
          className={`${baseBtn} ${generateBtnStyle}`}
        >
          <PlayIcon size={13} />
          Generate
        </Button>

        <div className={`flex-1 h-[5px] ${trackColor} overflow-hidden`}>
          <div
            className={`h-full ${fillColor} transition-[width] duration-200`}
            style={{ width: `${t.generateProgress}%` }}
          />
        </div>

        <Button
          type="button"
          onClick={t.onDownload}
          disabled={downloadDisabled}
          className={`${baseBtn} ${downloadBtnStyle}`}
        >
          <DownloadIcon size={13} />
          Download
        </Button>
      </div>

      {t.generateStatus === "error" && t.errors.length > 0 && (
        <div className="mt-[8px] bg-white border border-neutral-200 overflow-hidden">
          <table className="w-full text-[12px] border-collapse">
            <thead className="bg-neutral-50 text-neutral-500">
              <tr>
                <th className="text-left px-[10px] py-[5px] font-medium w-[55px]">Line</th>
                <th className="text-left px-[10px] py-[5px] font-medium w-[90px]">Type</th>
                <th className="text-left px-[10px] py-[5px] font-medium">Message</th>
              </tr>
            </thead>
            <tbody>
              {t.errors.map((err, i) => (
                <tr key={i} className="border-t border-neutral-200">
                  <td className="px-[10px] py-[5px] font-mono text-neutral-500">
                    {err.line ?? "—"}
                  </td>
                  <td className="px-[10px] py-[5px] font-mono text-neutral-900">
                    {err.errorType}
                  </td>
                  <td className="px-[10px] py-[5px] text-[#b91c1c]">{err.message}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export function StepGenerateFile({ transforms }: Props) {
  return (
    <div className="bg-neutral-50 border border-neutral-200 px-[12px] py-[10px] flex flex-col gap-[12px]">
      {transforms.map((t, i) => (
        <div key={i}>
          {i > 0 && (
            <div className="h-px bg-neutral-200 -mx-[12px] mb-[12px]" />
          )}
          <TransformBlock t={t} />
        </div>
      ))}
    </div>
  );
}
