// StepSelectFiles.tsx
//
// Renders steps with type=file_input.
// Layout: optional description, then a flat card with
// [Classification] [Upload] [filename pill] [Validate].

import {
  UploadIcon,
  ListChecksIcon,
  CheckIcon,
  XIcon,
} from "lucide-react";
import { Button } from "../ui/button";
import type { FileStatus } from "../../App";

type Props = {
  description: string;
  inputLabel: string;
  accepts: string[];
  fileName: string | null;
  fileStatus: FileStatus;
  onFileSelect: (path: string, name: string) => void;
  onValidate: () => void;
};

// Splits prose with single backticks into <code>…</code> spans for inline code.
function renderInlineMd(text: string) {
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          className="font-mono text-[12px] bg-neutral-100 px-[5px] py-[1px] rounded border border-neutral-200"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

export function StepSelectFiles({
  description,
  inputLabel,
  accepts: _accepts,
  fileName,
  fileStatus,
  onFileSelect,
  onValidate,
}: Props) {
  // accepts is consumed when the real file dialog is wired in
  void _accepts;

  const validateDisabled = fileStatus === "none";

  const baseBtn =
    "rounded-[10px] h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none active:translate-y-0";
  const activeBtn = "bg-[#1a1a1a] hover:bg-[#2a2a2a] text-white";
  const doneBtn =
    "bg-[#e0ddd8] hover:bg-[#e0ddd8] text-neutral-500 disabled:opacity-100";
  const notReadyBtn =
    "bg-neutral-200 hover:bg-neutral-200 text-neutral-400 disabled:opacity-100";

  const uploadBtnStyle = fileStatus === "none" ? activeBtn : doneBtn;

  const validateVisual =
    fileStatus === "valid"
      ? { className: "bg-[#bbf7d0] hover:bg-[#bbf7d0] text-[#15803d] disabled:opacity-100", Icon: CheckIcon }
      : fileStatus === "invalid"
      ? { className: "bg-[#fecaca] hover:bg-[#fecaca] text-[#b91c1c] disabled:opacity-100", Icon: XIcon }
      : fileStatus === "pending"
      ? { className: activeBtn, Icon: ListChecksIcon }
      : { className: notReadyBtn, Icon: ListChecksIcon };

  // Mock-phase: clicking Upload fakes a file selection. The real dialog
  // is wired in a later step.
  const handleUploadClick = () => {
    const fakeName = "file_from_vendor.csv";
    const fakePath = `/mock/${fakeName}`;
    onFileSelect(fakePath, fakeName);
  };

  return (
    <>
      {description && (
        <p className="text-[13px] text-neutral-500 leading-relaxed mb-[7px]">
          {renderInlineMd(description)}
        </p>
      )}
      <div className="bg-neutral-100 rounded-xl px-[12px] py-[8px]">
        <div className="flex items-center gap-[8px]">
          <span className="text-[12px] font-medium text-neutral-900 whitespace-nowrap">
            {inputLabel}
          </span>

          <Button
            type="button"
            onClick={handleUploadClick}
            className={`${baseBtn} ${uploadBtnStyle}`}
          >
            <UploadIcon size={13} />
            Upload
          </Button>

          <div
            className={
              "flex-1 h-[28px] flex items-center px-[9px] rounded-md border font-mono text-[12px] overflow-hidden text-ellipsis whitespace-nowrap bg-white " +
              (fileName
                ? "border-neutral-300 text-neutral-900"
                : "border-neutral-200 text-neutral-400")
            }
          >
            <span className="overflow-hidden text-ellipsis whitespace-nowrap">
              {fileName ?? "no file selected"}
            </span>
          </div>

          <Button
            type="button"
            onClick={onValidate}
            disabled={validateDisabled}
            className={`${baseBtn} ${validateVisual.className}`}
          >
            <validateVisual.Icon size={13} />
            Validate
          </Button>
        </div>
      </div>
    </>
  );
}
