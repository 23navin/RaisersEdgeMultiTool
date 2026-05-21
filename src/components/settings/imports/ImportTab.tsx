// ImportTab.tsx
//
// Orchestrator for the Import-profiles tab. Owns all state, runs every
// invoke() call, and threads handlers down to ProfileSidebar / ProfileEditor.
// The view children are pure — they don't talk to the backend themselves.

import { useEffect, useMemo, useRef, useState } from "react";
import { AlertCircleIcon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ProfileSummary,
  LoadedProfile,
  ProfileFileEntry,
  ProfileMutation,
  ValidationReport,
} from "../../../types";
import {
  buildAnchorMap,
  type AnchorMap,
  type NavTarget,
} from "../../shared/CodeMirrorEditor";
import { ProfileSidebar } from "./ProfileSidebar";
import { ProfileEditor } from "./ProfileEditor";
import type { EditorState } from "./types";
import { sortProfileFiles } from "./utils";

export function ImportTab({ panelOpen }: { panelOpen: boolean }) {
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [selectedZipPath, setSelectedZipPath] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [activePath, setActivePath] = useState<string>("structure.yaml");
  const [validationReport, setValidationReport] = useState<ValidationReport | null>(null);
  const [issuesCollapsed, setIssuesCollapsed] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [query, setQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [scrollTarget, setScrollTarget] = useState<{ line: number; nonce: number } | null>(null);

  // Ignore stale load responses when the user clicks profiles in quick succession.
  const loadReqId = useRef(0);
  const navNonce = useRef(0);

  // Rebuild the cross-file anchor map whenever the loaded file set changes.
  const anchors: AnchorMap = useMemo(
    () => (editor ? buildAnchorMap(editor.files) : buildAnchorMap([])),
    [editor],
  );

  const handleNavigate = (target: NavTarget) => {
    if (!editor) return;
    const exists = editor.files.some((f) => f.path === target.path);
    if (!exists) return;
    if (target.path !== activePath) setActivePath(target.path);
    setScrollTarget({ line: target.line, nonce: ++navNonce.current });
  };

  const refreshList = async (): Promise<ProfileSummary[]> => {
    const list = await invoke<ProfileSummary[]>("list_profiles");
    setProfiles(list);
    return list;
  };

  // Initial load when the tab mounts.
  useEffect(() => {
    refreshList().catch((e) => setError(String(e)));
  }, []);

  // Click a profile in the sidebar. Built-ins open read-only; the user must
  // press 'Duplicate to edit' to fork them into a writable user copy.
  const handleSelect = async (summary: ProfileSummary) => {
    const reqId = ++loadReqId.current;
    setSelectedZipPath(summary.zip_path);
    setEditor(null);
    setValidationReport(null);
    setDirty(false);
    setError(null);
    setBusy(true);

    try {
      const loaded = await invoke<LoadedProfile>("load_profile", {
        zipPath: summary.zip_path,
      });
      if (loadReqId.current !== reqId) return;
      const files = sortProfileFiles(loaded.files);
      setEditor({ summary, files });
      setActivePath(files[0]?.path ?? "structure.yaml");
    } catch (e) {
      if (loadReqId.current !== reqId) return;
      setError(String(e));
    } finally {
      if (loadReqId.current === reqId) setBusy(false);
    }
  };

  const updateActiveFile = (next: string) => {
    if (!editor) return;
    // Built-ins are immutable — silently ignore edits. The textarea is also
    // readOnly so this is a belt-and-braces guard.
    if (editor.summary.source === "builtin") return;
    setEditor({
      ...editor,
      files: editor.files.map((f) =>
        f.path === activePath ? { ...f, content: next } : f,
      ),
    });
    setDirty(true);
    setValidationReport(null);
  };

  const runValidate = async (files: ProfileFileEntry[]): Promise<ValidationReport | null> => {
    try {
      const report = await invoke<ValidationReport>("validate_profile", { files });
      setValidationReport(report);
      setIssuesCollapsed(false);
      return report;
    } catch (e) {
      setError(String(e));
      return null;
    }
  };

  const handleValidate = async () => {
    if (!editor) return;
    setBusy(true);
    setError(null);
    try {
      await runValidate(editor.files);
    } finally {
      setBusy(false);
    }
  };

  const handleScaffold = async () => {
    if (!editor) return;
    setBusy(true);
    setError(null);
    try {
      const files = await invoke<ProfileFileEntry[]>("scaffold_missing", {
        files: editor.files,
      });
      const sorted = sortProfileFiles(files);
      setEditor({ ...editor, files: sorted });
      setDirty(true);
      // Switch to the first file that's new or changed by the scaffold so the
      // user can see what was added.
      const previousPaths = new Set(editor.files.map((f) => f.path));
      const added = sorted.find((f) => !previousPaths.has(f.path));
      if (added) setActivePath(added.path);
      await runValidate(sorted);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleNewProfile = async () => {
    setBusy(true);
    setError(null);
    try {
      const mut = await invoke<ProfileMutation>("new_profile");
      await refreshList();
      const files = sortProfileFiles(mut.loaded.files);
      setEditor({ summary: mut.summary, files });
      setSelectedZipPath(mut.summary.zip_path);
      setActivePath(files[0]?.path ?? "structure.yaml");
      setDirty(false);
      setValidationReport(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDuplicate = async () => {
    if (!editor) return;
    setBusy(true);
    setError(null);
    try {
      const mut = await invoke<ProfileMutation>("duplicate_profile", {
        sourceZipPath: editor.summary.zip_path,
      });
      await refreshList();
      const files = sortProfileFiles(mut.loaded.files);
      setEditor({ summary: mut.summary, files });
      setSelectedZipPath(mut.summary.zip_path);
      setActivePath(files[0]?.path ?? "structure.yaml");
      setDirty(false);
      setValidationReport(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async () => {
    if (!editor) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("delete_profile", { zipPath: editor.summary.zip_path });
      await refreshList();
      setEditor(null);
      setSelectedZipPath(null);
      setDirty(false);
      setValidationReport(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const handleSave = async () => {
    if (!editor) return;
    // Soft gate: if the last validation reported errors, require confirmation.
    if (validationReport && validationReport.error_count > 0) {
      const ok = window.confirm(
        `Profile has ${validationReport.error_count} validation error(s). Save anyway?`,
      );
      if (!ok) return;
    }
    setBusy(true);
    setError(null);
    try {
      const mut = await invoke<ProfileMutation>("save_profile", {
        zipPath: editor.summary.zip_path,
        files: editor.files,
      });
      await refreshList();
      const files = sortProfileFiles(mut.loaded.files);
      setEditor({ summary: mut.summary, files });
      setSelectedZipPath(mut.summary.zip_path);
      setDirty(false);
      // Re-validate against the freshly-loaded files so the report reflects
      // exactly what's on disk now.
      await runValidate(files);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // Ctrl/Cmd-S: validate, then save only if there are no errors or warnings.
  // Surfaces the report so the user can act on issues instead of getting the
  // regular "save anyway?" confirm dialog from the keyboard shortcut.
  const handleValidateAndSave = async () => {
    if (!editor) return;
    if (editor.summary.source === "builtin") return;
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const report = await runValidate(editor.files);
      if (!report || report.error_count > 0 || report.warning_count > 0) return;
      const mut = await invoke<ProfileMutation>("save_profile", {
        zipPath: editor.summary.zip_path,
        files: editor.files,
      });
      await refreshList();
      const files = sortProfileFiles(mut.loaded.files);
      setEditor({ summary: mut.summary, files });
      setSelectedZipPath(mut.summary.zip_path);
      setDirty(false);
      await runValidate(files);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const shortcutRef = useRef(handleValidateAndSave);
  shortcutRef.current = handleValidateAndSave;

  // Capture Ctrl/Cmd-S while the settings panel is open and Import Profiles
  // is the active tab. Defers to a ref so the listener doesn't have to
  // re-register every render.
  useEffect(() => {
    if (!panelOpen) return;
    const onKey = (e: KeyboardEvent) => {
      const isSave = (e.metaKey || e.ctrlKey) && (e.key === "s" || e.key === "S");
      if (!isSave) return;
      e.preventDefault();
      e.stopPropagation();
      shortcutRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [panelOpen]);

  const activeFile = editor?.files.find((f) => f.path === activePath);

  return (
    <div className="h-full flex">
      <ProfileSidebar
        profiles={profiles}
        selectedZipPath={selectedZipPath}
        query={query}
        onQueryChange={setQuery}
        onSelect={handleSelect}
        onNewProfile={handleNewProfile}
        busy={busy}
      />

      <section className="flex-1 flex flex-col min-w-0">
        {editor ? (
          <ProfileEditor
            editor={editor}
            activePath={activePath}
            onActivePath={setActivePath}
            activeFile={activeFile}
            onChange={updateActiveFile}
            report={validationReport}
            issuesCollapsed={issuesCollapsed}
            onToggleIssues={() => setIssuesCollapsed((v) => !v)}
            dirty={dirty}
            busy={busy}
            error={error}
            anchors={anchors}
            scrollTarget={scrollTarget}
            onNavigate={handleNavigate}
            onValidate={handleValidate}
            onScaffold={handleScaffold}
            onSave={handleSave}
            onDuplicate={handleDuplicate}
            onDelete={handleDelete}
          />
        ) : (
          <div className="flex-1 flex flex-col">
            {error && (
              <div className="shrink-0 border-b border-neutral-200 bg-red-50 px-[16px] py-[8px] text-[12px] text-red-700 flex items-start gap-[6px]">
                <AlertCircleIcon size={14} className="shrink-0 mt-[2px]" />
                <span className="break-words">{error}</span>
              </div>
            )}
            <div className="flex-1 flex items-center justify-center text-[13px] text-neutral-500">
              {busy ? "Loading…" : "Select a profile to edit"}
            </div>
          </div>
        )}
      </section>
    </div>
  );
}
