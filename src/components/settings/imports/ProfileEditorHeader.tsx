// ProfileEditorHeader.tsx
//
// Top bar of the profile editor: title, version, read-only/dirty indicators,
// and the action button cluster (Duplicate / Delete / Validate / Save).
//
// Delete is two-click: first press arms the button (label/color change),
// second press fires onDelete. Auto-disarms after 3s of idle, on busy, on
// blur, or when the user switches to a different profile.

import { useEffect, useState } from "react";
import { CopyIcon, Trash2Icon } from "lucide-react";
import { cn } from "../../../lib/utils";
import type { ProfileSummary, ValidationReport } from "../../../types";

type Props = {
  summary: ProfileSummary;
  readOnly: boolean;
  dirty: boolean;
  busy: boolean;
  report: ValidationReport | null;
  onDuplicate: () => void;
  onDelete: () => void;
  onValidate: () => void;
  onSave: () => void;
};

export function ProfileEditorHeader({
  summary,
  readOnly,
  dirty,
  busy,
  report,
  onDuplicate,
  onDelete,
  onValidate,
  onSave,
}: Props) {
  const [deleteArmed, setDeleteArmed] = useState(false);

  // Disarm on profile switch.
  useEffect(() => {
    setDeleteArmed(false);
  }, [summary.zip_path]);
  // Auto-disarm 3s after the first click.
  useEffect(() => {
    if (!deleteArmed) return;
    const t = window.setTimeout(() => setDeleteArmed(false), 3000);
    return () => window.clearTimeout(t);
  }, [deleteArmed]);
  // Disarm if anything else starts loading.
  useEffect(() => {
    if (busy) setDeleteArmed(false);
  }, [busy]);

  const handleDeleteClick = () => {
    if (!deleteArmed) {
      setDeleteArmed(true);
      return;
    }
    setDeleteArmed(false);
    onDelete();
  };

  // Header
  //
  // Fixed min-height so the box doesn't jump when the "unsaved" line
  // toggles. The header centers its content vertically (items-center),
  // so when the inner column is just the title it sits in the middle;
  // when "unsaved" expands below the title, the pair stays centered as
  // a unit, which visually nudges the title upward.
  return (
    <header className="shrink-0 border-b border-neutral-200 px-[10px] py-[5px] min-h-[43px] flex items-center justify-between gap-[6px]">
      <div className="flex flex-col min-w-0">
        <div className="flex items-baseline gap-[4px] min-w-0">
          <span className="text-[14px] font-semibold text-neutral-900 truncate">
            {summary.name}
          </span>
          <span className="text-[11px] text-neutral-500">v{summary.version}</span>
          {readOnly && (
            <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 ml-[4px]">
              built-in
            </span>
          )}
        </div>
        <div
          aria-hidden={readOnly || !dirty}
          className={cn(
            "overflow-hidden transition-[max-height,opacity,margin-top] duration-200 ease-out",
            !readOnly && dirty
              ? "max-h-[20px] opacity-100 mt-[-5px]"
              : "max-h-0 opacity-0 mt-0",
          )}
        >
          <span className="text-[11px] text-amber-600 block">
            {report && (report.error_count > 0 || report.warning_count > 0)
              ? "validation issues"
              : report
                ? "unsaved changes"
                : "unvalidated changes"}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-[6px] shrink-0">
        <button
          type="button"
          onClick={onDuplicate}
          disabled={busy}
          title={readOnly ? "Create an editable copy" : "Duplicate this profile"}
          className={cn(
            "inline-flex items-center gap-[4px] text-[12px] px-[10px] py-[5px] rounded-[6px] cursor-pointer disabled:cursor-wait disabled:opacity-60 transition-colors",
            readOnly
              ? "bg-neutral-900 text-white border-0 hover:bg-neutral-800"
              : "border border-neutral-200 bg-white text-neutral-700 hover:bg-neutral-50",
          )}
        >
          <CopyIcon size={12} />
          {readOnly ? "Duplicate to edit" : "Duplicate"}
        </button>
        {!readOnly && (
          <>
            <button
              type="button"
              onClick={handleDeleteClick}
              onBlur={() => setDeleteArmed(false)}
              disabled={busy}
              title={deleteArmed ? "Click again to permanently delete" : "Delete this profile"}
              className={cn(
                "inline-flex items-center gap-[4px] text-[12px] px-[10px] py-[5px] rounded-[6px] cursor-pointer disabled:cursor-wait disabled:opacity-60 transition-colors",
                deleteArmed
                  ? "border border-red-600 bg-red-600 text-white hover:bg-red-700"
                  : "border border-neutral-200 bg-white text-red-700 hover:bg-red-50",
              )}
            >
              <Trash2Icon size={12} />
              Delete
            </button>
            <button
              type="button"
              onClick={onValidate}
              disabled={busy}
              className="text-[12px] px-[10px] py-[5px] rounded-[6px] border border-neutral-200 bg-white text-neutral-700 hover:bg-neutral-50 cursor-pointer disabled:cursor-wait disabled:opacity-60"
            >
              Validate
            </button>
            <button
              type="button"
              onClick={onSave}
              disabled={!dirty || busy}
              className={cn(
                "text-[12px] px-[10px] py-[5px] rounded-[6px] border-0 cursor-pointer transition-colors",
                dirty && !busy
                  ? "bg-neutral-900 text-white hover:bg-neutral-800"
                  : "bg-neutral-200 text-neutral-400 cursor-not-allowed",
              )}
            >
              Save
            </button>
          </>
        )}
      </div>
    </header>
  );
}
