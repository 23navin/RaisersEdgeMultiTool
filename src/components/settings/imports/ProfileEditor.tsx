// ProfileEditor.tsx
//
// Right-hand pane of the Import-profiles tab: header + file tree +
// CodeMirror editor + issues footer. Pure view — all state and invoke()
// calls live in the parent ImportTab.

import { useMemo } from "react";
import { AlertCircleIcon } from "lucide-react";
import type { ProfileFileEntry, ValidationReport } from "../../../types";
import {
  CodeMirrorEditor,
  issueToFileLine,
  type AnchorMap,
  type EditorIssue,
  type NavTarget,
} from "../../shared/CodeMirrorEditor";
import { FileTree } from "./FileTree";
import { IssuesPanel } from "./IssuesPanel";
import { ProfileEditorHeader } from "./ProfileEditorHeader";
import type { EditorState } from "./types";

type Props = {
  editor: EditorState;
  activePath: string;
  onActivePath: (p: string) => void;
  activeFile: ProfileFileEntry | undefined;
  onChange: (next: string) => void;
  report: ValidationReport | null;
  issuesCollapsed: boolean;
  onToggleIssues: () => void;
  dirty: boolean;
  busy: boolean;
  error: string | null;
  anchors: AnchorMap;
  scrollTarget: { line: number; nonce: number } | null;
  onNavigate: (target: NavTarget) => void;
  onValidate: () => void;
  onScaffold: () => void;
  onSave: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
};

export function ProfileEditor({
  editor,
  activePath,
  onActivePath,
  activeFile,
  onChange,
  report,
  issuesCollapsed,
  onToggleIssues,
  dirty,
  busy,
  error,
  anchors,
  scrollTarget,
  onNavigate,
  onValidate,
  onScaffold,
  onSave,
  onDuplicate,
  onDelete,
}: Props) {
  const { summary } = editor;
  const readOnly = summary.source === "builtin";

  // Squiggle decorations for whichever file is currently open. Locations
  // outside the active file are filtered out before reaching CodeMirror.
  const activeFileIssues = useMemo<EditorIssue[]>(() => {
    if (!report) return [];
    const out: EditorIssue[] = [];
    for (const issue of report.issues) {
      const target = issueToFileLine(issue.location, anchors);
      if (!target || target.path !== activePath) continue;
      out.push({ line: target.line, severity: issue.severity, message: issue.message });
    }
    return out;
  }, [report, anchors, activePath]);

  return (
    <>
      <ProfileEditorHeader
        summary={summary}
        readOnly={readOnly}
        dirty={dirty}
        busy={busy}
        report={report}
        onDuplicate={onDuplicate}
        onDelete={onDelete}
        onValidate={onValidate}
        onSave={onSave}
      />

      <div className="flex-1 flex min-h-0">
        <FileTree
          files={editor.files}
          activePath={activePath}
          onSelect={onActivePath}
        />
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          {activeFile ? (
            <CodeMirrorEditor
              key={`${summary.zip_path}:${activeFile.path}`}
              path={activeFile.path}
              value={activeFile.content}
              onChange={onChange}
              readOnly={readOnly}
              anchors={anchors}
              onNavigate={onNavigate}
              scrollToLine={scrollTarget}
              issues={activeFileIssues}
            />
          ) : (
            <div className="flex-1 flex items-center justify-center text-[13px] text-neutral-500">
              Select a file
            </div>
          )}
        </div>
      </div>

      {error && (
        <div className="shrink-0 border-t border-neutral-200 bg-red-50 px-[16px] py-[8px] text-[12px] text-red-700 flex items-center gap-[6px]">
          <AlertCircleIcon size={14} />
          {error}
        </div>
      )}
      {report && !readOnly && (
        <IssuesPanel
          report={report}
          collapsed={issuesCollapsed}
          onToggle={onToggleIssues}
          onScaffold={onScaffold}
          onNavigate={onNavigate}
          anchors={anchors}
          busy={busy}
        />
      )}
    </>
  );
}
