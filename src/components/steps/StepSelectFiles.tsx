// StepSelectFiles.tsx
//
// Renders steps with type=file_input.
// One step's card can contain multiple upload rows (one per input).
// Each row: [InputLabel] [Upload] [filename pill] [Validate]
// Errors table renders below the row when fileStatus === "invalid".

import {
  UploadIcon,
  ListChecksIcon,
  CheckIcon,
  XIcon,
} from "lucide-react";
import { Button } from "../ui/button";
import type { FileStatus } from "../../App";
import type { ValidationError } from "../../types";

export type FileInputRow = {
  inputLabel: string;
  fileName: string | null;
  fileStatus: FileStatus;
  errors: ValidationError[];
};

type Props = {
  description: string;
  rows: FileInputRow[];
  onFileSelect: (inputLabel: string, path: string, name: string) => void;
  onValidate: (inputLabel: string) => void;
  onClear: (inputLabel: string) => void;
};

// Splits prose with single backticks into <code>…</code> spans for inline code.
function renderInlineMd(text: string) {
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`")) {
      return (
        <code
          key={i}
          className="font-mono text-[12px] bg-neutral-100 px-[5px] py-[1px] border border-neutral-200"
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

const baseBtn =
  "rounded-none h-[32px] px-[14px] text-[13px] font-medium border-0 shadow-none transform-gpu";
const activeBtn =
  "bg-[#1a1a1a] hover:bg-[#2a2a2a] text-white cursor-pointer";
const doneBtn =
  "bg-[#e0ddd8] hover:bg-[#d4d0c9] text-neutral-500 hover:text-neutral-700 disabled:opacity-100 cursor-pointer";
const notReadyBtn =
  "bg-neutral-200 hover:bg-neutral-200 text-neutral-400 disabled:opacity-100 cursor-not-allowed";

function FileRow({
  row,
  onFileSelect,
  onValidate,
  onClear,
}: {
  row: FileInputRow;
  onFileSelect: (inputLabel: string, path: string, name: string) => void;
  onValidate: (inputLabel: string) => void;
  onClear: (inputLabel: string) => void;
}) {
  const { inputLabel, fileName, fileStatus, errors } = row;
  const validateDisabled = fileStatus === "none";
  const uploadBtnStyle = fileStatus === "none" ? activeBtn : doneBtn;

  const validateVisual =
    fileStatus === "valid"
      ? { className: "bg-[#bbf7d0] hover:bg-[#a7ebbf] text-[#15803d] disabled:opacity-100 cursor-pointer", Icon: CheckIcon }
      : fileStatus === "invalid"
      ? { className: "bg-[#fecaca] hover:bg-[#f9b9b9] text-[#b91c1c] disabled:opacity-100 cursor-pointer", Icon: XIcon }
      : fileStatus === "pending"
      ? { className: activeBtn, Icon: ListChecksIcon }
      : { className: notReadyBtn, Icon: ListChecksIcon };

  // Mock-phase: clicking Upload fakes a file selection.
  const handleUploadClick = () => {
    const fakeName = "file_from_vendor.csv";
    const fakePath = `/mock/${fakeName}`;
    onFileSelect(inputLabel, fakePath, fakeName);
  };

  return (
    <div>
      <div className="flex items-center gap-[8px]">
        <span className="text-[12px] font-medium text-neutral-900 whitespace-nowrap min-w-[80px]">
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
            "flex-1 h-[28px] flex items-center gap-[6px] pl-[9px] pr-[4px] border font-mono text-[12px] bg-white " +
            (fileName
              ? "border-neutral-300 text-neutral-900"
              : "border-neutral-200 text-neutral-400")
          }
        >
          <span className="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap">
            {fileName ?? "no file selected"}
          </span>
          {fileName && (
            <button
              type="button"
              onClick={() => onClear(inputLabel)}
              aria-label="Clear file"
              className="shrink-0 inline-flex items-center justify-center h-[20px] w-[20px] text-neutral-400 hover:text-neutral-700 hover:bg-neutral-100 cursor-pointer"
            >
              <XIcon size={13} />
            </button>
          )}
        </div>

        <Button
          type="button"
          onClick={() => onValidate(inputLabel)}
          disabled={validateDisabled}
          className={`${baseBtn} ${validateVisual.className}`}
        >
          <validateVisual.Icon size={13} />
          Validate
        </Button>
      </div>

      {fileStatus === "invalid" && errors.length > 0 && (
        <div className="mt-[8px] bg-white border border-neutral-200 overflow-hidden">
          <table className="w-full text-[12px] border-collapse">
            <thead className="bg-neutral-50 text-neutral-500">
              <tr>
                <th className="text-left px-[10px] py-[5px] font-medium w-[50px]">Row</th>
                <th className="text-left px-[10px] py-[5px] font-medium">Column</th>
                <th className="text-left px-[10px] py-[5px] font-medium">Value</th>
                <th className="text-left px-[10px] py-[5px] font-medium">Error</th>
              </tr>
            </thead>
            <tbody>
              {errors.map((err, i) => (
                <tr key={i} className="border-t border-neutral-200">
                  <td className="px-[10px] py-[5px] font-mono text-neutral-500">{err.row}</td>
                  <td className="px-[10px] py-[5px] font-mono text-neutral-900">{err.column}</td>
                  <td className="px-[10px] py-[5px] font-mono text-neutral-900">{err.value}</td>
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

export function StepSelectFiles({
  description,
  rows,
  onFileSelect,
  onValidate,
  onClear,
}: Props) {
  return (
    <>
      {description && (
        <p className="text-[13px] text-neutral-500 leading-relaxed mb-[7px]">
          {renderInlineMd(description)}
        </p>
      )}
      <div className="bg-neutral-100 px-[12px] py-[8px] flex flex-col gap-[8px]">
        {rows.map((row) => (
          <FileRow
            key={row.inputLabel}
            row={row}
            onFileSelect={onFileSelect}
            onValidate={onValidate}
            onClear={onClear}
          />
        ))}
      </div>
    </>
  );
}
