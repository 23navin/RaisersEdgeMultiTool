// StepGenerateFile.tsx
//
// Renders steps with type=sql_transform.
// Card contains the pipeline diagram (inputs → DB → outputs) and a
// generate row (Generate / progress / Download).

import {
  DatabaseIcon,
  FileTextIcon,
  TableIcon,
  PlayIcon,
  DownloadIcon,
  type LucideIcon,
} from "lucide-react";
import { Button } from "../ui/button";
import type { GenerateStatus } from "../../App";

type Props = {
  inputs: string[];
  outputs: string[];
  canGenerate: boolean;
  generateStatus: GenerateStatus;
  generateProgress: number;
  onGenerate: () => void;
  onDownload: () => void;
};

function PipeNode({ icon: Icon, label }: { icon: LucideIcon; label: string }) {
  return (
    <div className="inline-flex items-center gap-[5px] px-[10px] py-[4px] rounded-md bg-white border border-neutral-200 text-neutral-500 text-[11px] whitespace-nowrap shadow-sm">
      <Icon size={12} />
      <span className="overflow-hidden text-ellipsis">{label}</span>
    </div>
  );
}

function ArrowSVG() {
  // The line fills available width via flex; the arrowhead is a fixed-size
  // SVG so it doesn't distort when the window is resized.
  return (
    <div className="flex items-center w-full">
      <div className="flex-1 h-px bg-[#d1d5db]" />
      <svg width="9" height="12" viewBox="0 0 9 12" className="shrink-0">
        <polygon points="0,2 9,6 0,10" fill="#d1d5db" />
      </svg>
    </div>
  );
}

export function StepGenerateFile({
  inputs,
  outputs,
  canGenerate,
  generateStatus,
  generateProgress,
  onGenerate,
  onDownload,
}: Props) {
  const generateDisabled = !canGenerate || generateStatus === "running";
  const downloadDisabled = generateStatus !== "done";

  const fillColor =
    generateStatus === "done" ? "bg-green-500" : "bg-neutral-400";

  return (
    <div className="bg-neutral-100 rounded-xl px-[12px] py-[10px]">
      {/* Pipeline diagram */}
      <div className="flex items-center w-full mb-[10px]">
        <div className="flex flex-col gap-[5px] shrink-0">
          {inputs.map((label) => (
            <PipeNode key={label} icon={FileTextIcon} label={label} />
          ))}
        </div>

        <div className="flex-1 flex items-center min-w-[20px] px-[4px]">
          <ArrowSVG />
        </div>

        <div className="shrink-0 px-[4px]">
          <DatabaseIcon size={22} className="text-neutral-400" />
        </div>

        <div className="flex-1 flex items-center min-w-[20px] px-[4px]">
          <ArrowSVG />
        </div>

        <div className="flex flex-col gap-[5px] shrink-0">
          {outputs.map((label) => (
            <PipeNode key={label} icon={TableIcon} label={label} />
          ))}
        </div>
      </div>

      {/* Generate row */}
      <div className="flex items-center gap-[8px]">
        <Button
          type="button"
          onClick={onGenerate}
          disabled={generateDisabled}
          className="bg-[#1a1a1a] hover:bg-[#2a2a2a] text-white rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none active:translate-y-0"
        >
          <PlayIcon size={13} />
          Generate
        </Button>

        <div className="flex-1 h-[5px] rounded-full bg-white overflow-hidden">
          <div
            className={`h-full ${fillColor} transition-[width] duration-200`}
            style={{ width: `${generateProgress}%` }}
          />
        </div>

        <Button
          type="button"
          onClick={onDownload}
          disabled={downloadDisabled}
          className="bg-[#e0ddd8] hover:bg-[#d4d0c9] text-neutral-900 rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none active:translate-y-0"
        >
          <DownloadIcon size={13} />
          Download
        </Button>
      </div>
    </div>
  );
}
