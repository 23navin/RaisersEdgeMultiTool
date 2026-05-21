// IssuesPanel.tsx
//
// Collapsible footer below the editor listing each validation issue with
// severity + locator. Clicking an issue navigates to the corresponding line
// in the editor (parent handles the navigation via `onNavigate`).

import type { ReactNode } from "react";
import { AlertCircleIcon } from "lucide-react";
import { cn } from "../../../lib/utils";
import type { ValidationReport, ValidationIssue } from "../../../types";
import {
  issueToFileLine,
  type AnchorMap,
  type NavTarget,
} from "../../shared/CodeMirrorEditor";

type Props = {
  report: ValidationReport;
  collapsed: boolean;
  onToggle: () => void;
  onScaffold: () => void;
  onNavigate: (target: NavTarget) => void;
  anchors: AnchorMap;
  busy: boolean;
};

export function IssuesPanel({
  report,
  collapsed,
  onToggle,
  onScaffold,
  onNavigate,
  anchors,
  busy,
}: Props) {
  const parts: ReactNode[] = [];
  if (report.error_count > 0) {
    parts.push(
      <span key="errors" className="text-red-700">
        {report.error_count} error{report.error_count === 1 ? "" : "s"}
      </span>,
    );
  }
  if (report.warning_count > 0) {
    parts.push(
      <span key="warnings" className="text-amber-700">
        {report.warning_count} warning{report.warning_count === 1 ? "" : "s"}
      </span>,
    );
  }
  if (report.info_count > 0) {
    parts.push(<span key="info">{report.info_count} info</span>);
  }

  return (
    <div className="shrink-0 border-t border-neutral-200 bg-white max-h-[40%] flex flex-col">
      <div className="px-[16px] py-[6px] flex items-center justify-between border-b border-neutral-100 bg-neutral-50/60">
        <span className="text-[12px] text-green-800">
          {parts.length === 0
            ? "Profile is valid"
            : parts.map((p, i) => (
                <span key={i}>
                  {i > 0 && <span className="text-neutral-400"> · </span>}
                  {p}
                </span>
              ))}
        </span>
        <div className="flex items-center gap-[6px]">
          {report.fixable_count > 0 && (
            <button
              type="button"
              onClick={onScaffold}
              disabled={busy}
              className="inline-flex items-center gap-[4px] text-[11px] px-[8px] py-[3px] rounded-[5px] border border-neutral-200 bg-white text-neutral-800 hover:bg-neutral-50 cursor-pointer disabled:cursor-wait disabled:opacity-60"
            >
              Scaffold {report.fixable_count} missing item{report.fixable_count === 1 ? "" : "s"}
            </button>
          )}
          <button
            type="button"
            onClick={onToggle}
            className="text-[11px] text-neutral-500 hover:text-neutral-800 bg-transparent border-0 cursor-pointer"
          >
            {collapsed ? "Show" : "Hide"}
          </button>
        </div>
      </div>
      {!collapsed && report.issues.length > 0 && (
        <ul className="flex-1 overflow-auto m-0 p-0 list-none">
          {report.issues.map((issue, idx) => (
            <IssueRow
              key={`${issue.code}-${idx}`}
              issue={issue}
              anchors={anchors}
              onNavigate={onNavigate}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function IssueRow({
  issue,
  anchors,
  onNavigate,
}: {
  issue: ValidationIssue;
  anchors: AnchorMap;
  onNavigate: (target: NavTarget) => void;
}) {
  const target = issueToFileLine(issue.location, anchors);
  const palette =
    issue.severity === "error"
      ? "text-red-700"
      : issue.severity === "warning"
        ? "text-amber-700"
        : "text-neutral-600";
  const icon =
    issue.severity === "info"
      ? null
      : <AlertCircleIcon size={12} className={cn("shrink-0 mt-[3px]", palette)} />;

  const locator = target ? `${target.path}:${target.line}` : "";
  const clickable = !!target;

  return (
    <li className="border-b border-neutral-100 last:border-b-0">
      <button
        type="button"
        disabled={!clickable}
        onClick={() => target && onNavigate(target)}
        className={cn(
          "w-full text-left flex items-start gap-[6px] px-[16px] py-[6px] text-[12px] border-0 bg-transparent",
          clickable ? "hover:bg-black/5 cursor-pointer" : "cursor-default",
        )}
      >
        {icon}
        <span className="flex-1 min-w-0">
          <span className={cn("font-medium", palette)}>{issue.message}</span>
          {locator && (
            <span className="ml-[6px] text-[11px] text-neutral-400">· {locator}</span>
          )}
        </span>
        <span className="text-[10px] uppercase tracking-[0.04em] text-neutral-400 shrink-0">
          {issue.code}
        </span>
      </button>
    </li>
  );
}
