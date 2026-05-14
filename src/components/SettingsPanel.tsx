// SettingsPanel.tsx
//
// Floating settings card with a fullscreen backdrop.
// Backdrop is `fixed inset-0` so it blurs the entire window (titlebar +
// bg + body). The card itself is positioned `absolute inset-[28px]`
// within the body container, so it reads as a floating panel.
// Always mounted so it can animate in/out; visibility driven by `open`.

import { useEffect, useMemo, useRef, useState } from "react";
import {
  CheckCircle2Icon,
  AlertCircleIcon,
  FilePlusIcon,
  Trash2Icon,
  CopyIcon,
  SearchIcon,
  XIcon,
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { cn } from "../lib/utils";
import type {
  ProfileSummary,
  LoadedProfile,
  ProfileFileEntry,
  ProfileMutation,
  ValidationReport,
  ValidationIssue,
} from "../types";
import {
  CodeMirrorEditor,
  buildAnchorMap,
  issueToFileLine,
  type AnchorMap,
  type NavTarget,
} from "./CodeMirrorEditor";

type Props = {
  open: boolean;
  onClose: () => void;
};

type Tab = "general" | "import";

const TABS: { id: Tab; label: string }[] = [
  { id: "general", label: "General" },
  { id: "import", label: "Import Profiles" },
];

export function SettingsPanel({ open, onClose }: Props) {
  const [tab, setTab] = useState<Tab>("general");

  // Close on Escape.
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  return (
    <>
      {/* Backdrop — fixed so it blurs the entire window */}
      <div
        onClick={onClose}
        aria-hidden={!open}
        className={cn(
          "fixed inset-0 z-40 transition-[opacity,backdrop-filter] duration-200 ease-out",
          open
            ? "opacity-100 backdrop-blur-0 bg-black/20 pointer-events-auto"
            : "opacity-0 backdrop-blur-0 bg-transparent pointer-events-none",
        )}
      />

      {/* Card — inset within the body container so it reads as a floating panel */}
      <div
        aria-hidden={!open}
        className={cn(
          "absolute inset-[28px] z-50 flex flex-col bg-white rounded-[12px] border border-neutral-200 shadow-2xl overflow-hidden origin-center",
          "transition-[opacity,filter,transform] duration-200 ease-out",
          open
            ? "opacity-100 blur-0 scale-100 pointer-events-auto"
            : "opacity-0 blur-md scale-[1.04] pointer-events-none",
        )}
      >
        {/* Close button — absolute so the tab row reads as the primary header */}
        <button
          type="button"
          aria-label="Close settings"
          onClick={onClose}
          className="absolute top-[5px] right-[5px] w-[26px] h-[26px] rounded-[6px] inline-flex items-center justify-center text-neutral-500 border-0 bg-transparent hover:bg-black/5 z-10"
        >
          <XIcon size={15} />
        </button>

        {/* Tab bar */}
        <div className="shrink-0 border-b border-neutral-200 px-[15px]">
          <nav
            role="tablist"
            aria-label="Settings sections"
            className="flex gap-[10px] pt-[8px]"
          >
            {TABS.map((t) => (
              <TabButton
                key={t.id}
                label={t.label}
                active={tab === t.id}
                onClick={() => setTab(t.id)}
              />
            ))}
          </nav>
        </div>

        {/* Tab body */}
        <div className="flex-1 overflow-auto">
          {tab === "general" && <GeneralTab />}
          {tab === "import" && <ImportTab panelOpen={open} />}
        </div>
      </div>
    </>
  );
}

function TabButton({
  label,
  active,
  onClick,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "relative bg-transparent border-0 px-[2px] pb-[7px] text-[14px] tracking-[-0.01em] transition-colors cursor-pointer",
        active
          ? "text-neutral"
          : "text-neutral-400 hover:text-neutral-600",
      )}
    >
      {label}
      <span
        aria-hidden
        className={cn(
          "absolute left-0 right-0 -bottom-px h-[2px] bg-neutral-900 rounded-full transition-opacity",
          active ? "opacity-100" : "opacity-0",
        )}
      />
    </button>
  );
}

function GeneralTab() {
  return <div className="px-[24px] py-[16px]" />;
}

// ---------------------------------------------------------------------------
// Import-profile editor
// ---------------------------------------------------------------------------

type EditorState = {
  summary: ProfileSummary;
  files: ProfileFileEntry[];
};

function ImportTab({ panelOpen }: { panelOpen: boolean }) {
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

  // cmd/ctrl-click handler from CodeMirrorEditor.
  const handleNavigate = (target: NavTarget) => {
    if (!editor) return;
    const exists = editor.files.some((f) => f.path === target.path);
    if (!exists) return;
    if (target.path !== activePath) setActivePath(target.path);
    setScrollTarget({ line: target.line, nonce: ++navNonce.current });
  };

  const filteredProfiles = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return profiles;
    return profiles.filter(
      (p) =>
        p.name.toLowerCase().includes(q) || p.id.toLowerCase().includes(q),
    );
  }, [profiles, query]);

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

  // Ctrl/Cmd-S: validate, then save only if there are no errors. Surfaces
  // the report so the user can act on issues instead of getting the regular
  // "save anyway?" confirm dialog from the keyboard shortcut.
  const handleValidateAndSave = async () => {
    if (!editor) return;
    if (editor.summary.source === "builtin") return;
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      const report = await runValidate(editor.files);
      if (!report || report.error_count > 0) return;
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
      {/* Profile list */}
      <aside className="w-[220px] shrink-0 border-r border-neutral-200 flex flex-col bg-neutral-50/60">
        <div className="px-[8px] pt-[8px] pb-[4px]">
          <div className="relative">
            <SearchIcon
              size={12}
              className="absolute left-[8px] top-1/2 -translate-y-1/2 text-neutral-400 pointer-events-none"
            />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search profiles"
              className="w-full text-[12px] pl-[24px] pr-[24px] py-[5px] rounded-[6px] border border-neutral-200 bg-white text-neutral-800 placeholder:text-neutral-400 outline-none focus:border-neutral-400"
            />
            {query && (
              <button
                type="button"
                onClick={() => setQuery("")}
                aria-label="Clear search"
                className="absolute right-[4px] top-1/2 -translate-y-1/2 w-[18px] h-[18px] rounded-[4px] inline-flex items-center justify-center text-neutral-400 hover:text-neutral-700 hover:bg-black/5 border-0 bg-transparent cursor-pointer"
              >
                <XIcon size={12} />
              </button>
            )}
          </div>
        </div>
        <ul className="flex-1 overflow-auto p-[6px] pt-[6px] flex flex-col gap-[5px] list-none m-0">
          {filteredProfiles.length === 0 && (
            <li className="px-[10px] py-[10px] text-[12px] text-neutral-400 text-center">
              No matches
            </li>
          )}
          {filteredProfiles.map((p) => {
            const isSelected = p.zip_path === selectedZipPath;
            return (
              <li key={p.zip_path}>
                <button
                  type="button"
                  onClick={() => handleSelect(p)}
                  disabled={busy}
                  className={cn(
                    "w-full text-left px-[10px] py-[6px] rounded-[6px] border-0 transition-colors cursor-pointer disabled:cursor-wait",
                    isSelected
                      ? "bg-neutral-200/70"
                      : "bg-transparent hover:bg-black/5",
                  )}
                >
                  <div className="flex items-center justify-between gap-[6px]">
                    <span
                      className={cn(
                        "text-[13px] text-neutral-900 truncate",
                        isSelected ? "font-medium" : "font-normal",
                      )}
                    >
                      {p.name}
                    </span>
                    {p.source === "builtin" && (
                      <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
                        built-in
                      </span>
                    )}
                  </div>
                  <div className="text-[11px] text-neutral-500 mt-[1px]">
                    {p.id} v{p.version}
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
        <button
          type="button"
          onClick={handleNewProfile}
          disabled={busy}
          className="m-[8px] px-[10px] py-[7px] rounded-[7px] border border-dashed border-neutral-300 text-[12px] text-neutral-600 bg-transparent hover:bg-black/5 inline-flex items-center justify-center gap-[6px] cursor-pointer disabled:cursor-wait disabled:opacity-60"
        >
          <FilePlusIcon size={13} />
          New profile
        </button>
      </aside>

      {/* Editor pane */}
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

// structure.yaml first, then instructions.md, then everything else alphabetically.
function sortProfileFiles(files: ProfileFileEntry[]): ProfileFileEntry[] {
  const rank = (p: string) => {
    if (p === "structure.yaml") return 0;
    if (p === "instructions.md") return 1;
    return 2;
  };
  return [...files].sort((a, b) => {
    const r = rank(a.path) - rank(b.path);
    return r !== 0 ? r : a.path.localeCompare(b.path);
  });
}

function ProfileEditor({
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
}: {
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
}) {
  const fileGroups = useMemo(() => buildFileGroups(editor.files), [editor.files]);
  const { summary } = editor;
  const readOnly = summary.source === "builtin";

  // Two-click delete: first press arms the button (label becomes "Confirm"),
  // second press fires onDelete. Auto-disarms after 3s of idle, on busy, or
  // when the user switches to a different profile.
  const [deleteArmed, setDeleteArmed] = useState(false);
  useEffect(() => {
    setDeleteArmed(false);
  }, [summary.zip_path]);
  useEffect(() => {
    if (!deleteArmed) return;
    const t = window.setTimeout(() => setDeleteArmed(false), 3000);
    return () => window.clearTimeout(t);
  }, [deleteArmed]);
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

  return (
    <>
      {/* Header
       *
       * Fixed min-height so the box doesn't jump when the "unsaved" line
       * toggles. The header centers its content vertically (items-center),
       * so when the inner column is just the title it sits in the middle;
       * when "unsaved" expands below the title, the pair stays centered as
       * a unit, which visually nudges the title upward. */}
      <header className="shrink-0 border-b border-neutral-200 px-[16px] py-[10px] min-h-[58px] flex items-center justify-between gap-[12px]">
        <div className="flex flex-col min-w-0">
          <div className="flex items-baseline gap-[8px] min-w-0">
            <span className="text-[14px] font-semibold text-neutral-900 truncate">
              {summary.name}
            </span>
            <span className="text-[11px] text-neutral-500">v{summary.version}</span>
            {readOnly && (
              <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 ml-[4px]">
                built-in · read-only
              </span>
            )}
          </div>
          <div
            aria-hidden={readOnly || !dirty}
            className={cn(
              "overflow-hidden transition-[max-height,opacity,margin-top] duration-200 ease-out",
              !readOnly && dirty
                ? "max-h-[20px] opacity-100 mt-[1px]"
                : "max-h-0 opacity-0 mt-0",
            )}
          >
            <span className="text-[11px] text-amber-600 block">
              • unsaved changes
            </span>
          </div>
        </div>
        <div className="flex items-center gap-[6px] shrink-0">
          {!readOnly && <ValidationPill report={report} />}
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

      {/* Body: file tree + editor */}
      <div className="flex-1 flex min-h-0">
        <nav className="w-[180px] shrink-0 border-r border-neutral-200 overflow-auto py-[6px]">
          {fileGroups.map((group, idx) => {
            if (group.length === 0) return null;
            const hasPrev = fileGroups.slice(0, idx).some((g) => g.length > 0);
            return (
              <div key={idx}>
                {hasPrev && (
                  <div className="my-[6px] mx-[10px] border-t border-neutral-200" />
                )}
                {group.map((f) => (
                  <FileItem
                    key={f.path}
                    file={f}
                    active={activePath === f.path}
                    onSelect={onActivePath}
                  />
                ))}
              </div>
            );
          })}
        </nav>
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
            />
          ) : (
            <div className="flex-1 flex items-center justify-center text-[13px] text-neutral-500">
              Select a file
            </div>
          )}
        </div>
      </div>

      {/* Issues panel — appears after Validate or when a backend error occurs */}
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

function ValidationPill({ report }: { report: ValidationReport | null }) {
  if (!report) {
    return <span className="text-[11px] text-neutral-400">not validated</span>;
  }
  if (report.error_count > 0) {
    return (
      <span className="inline-flex items-center gap-[4px] text-[11px] text-red-700 bg-red-50 border border-red-200 px-[7px] py-[2px] rounded-[10px]">
        <AlertCircleIcon size={12} />
        {report.error_count} error{report.error_count === 1 ? "" : "s"}
      </span>
    );
  }
  if (report.warning_count > 0) {
    return (
      <span className="inline-flex items-center gap-[4px] text-[11px] text-amber-700 bg-amber-50 border border-amber-200 px-[7px] py-[2px] rounded-[10px]">
        <AlertCircleIcon size={12} />
        {report.warning_count} warning{report.warning_count === 1 ? "" : "s"}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-[4px] text-[11px] text-emerald-700 bg-emerald-50 border border-emerald-200 px-[7px] py-[2px] rounded-[10px]">
      <CheckCircle2Icon size={12} />
      Valid
    </span>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// IssuesPanel — collapsible footer listing each issue with severity + locator
// ─────────────────────────────────────────────────────────────────────────────

function IssuesPanel({
  report,
  collapsed,
  onToggle,
  onScaffold,
  onNavigate,
  anchors,
  busy,
}: {
  report: ValidationReport;
  collapsed: boolean;
  onToggle: () => void;
  onScaffold: () => void;
  onNavigate: (target: NavTarget) => void;
  anchors: AnchorMap;
  busy: boolean;
}) {
  const summary = [
    report.error_count > 0 && `${report.error_count} error${report.error_count === 1 ? "" : "s"}`,
    report.warning_count > 0 && `${report.warning_count} warning${report.warning_count === 1 ? "" : "s"}`,
    report.info_count > 0 && `${report.info_count} info`,
  ].filter(Boolean).join(" · ") || "Profile is valid";

  return (
    <div className="shrink-0 border-t border-neutral-200 bg-white max-h-[40%] flex flex-col">
      <div className="px-[16px] py-[6px] flex items-center justify-between border-b border-neutral-100 bg-neutral-50/60">
        <span className="text-[12px] text-neutral-700">{summary}</span>
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

  const locator = target
    ? `${target.path}:${target.line}`
    : "";

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

// ---------------------------------------------------------------------------
// File groups
//
// Folder structure is hidden from the user. Files are bucketed into three
// fixed groups: profile (structure.yaml + instructions.md), transform SQL,
// and notice SQL. Notice SQL is identified by parsing `notices:` references
// out of structure.yaml; everything else under sql/ is treated as transform.
// ---------------------------------------------------------------------------

function buildFileGroups(files: ProfileFileEntry[]): ProfileFileEntry[][] {
  const yaml = files.find((f) => f.path === "structure.yaml");
  const yamlContent = yaml?.content ?? "";

  const noticeRefs = new Set<string>();
  const blocks = yamlContent.matchAll(
    /notices:\s*\n((?:[ \t]+-[ \t]+[\w./-]+\s*\n?)+)/g,
  );
  for (const block of blocks) {
    const body = block[1] ?? "";
    for (const item of body.matchAll(/-[ \t]+([\w./-]+)/g)) {
      noticeRefs.add(item[1].replace(/^sql\//, ""));
    }
  }

  const profileFiles: ProfileFileEntry[] = [];
  const transformFiles: ProfileFileEntry[] = [];
  const noticeFiles: ProfileFileEntry[] = [];

  for (const f of files) {
    if (f.path === "structure.yaml" || f.path === "instructions.md") {
      profileFiles.push(f);
    } else {
      const name = f.path.split("/").pop() ?? f.path;
      if (f.path.endsWith(".sql") && noticeRefs.has(name)) {
        noticeFiles.push(f);
      } else {
        transformFiles.push(f);
      }
    }
  }

  return [profileFiles, transformFiles, noticeFiles];
}

function FileItem({
  file,
  active,
  onSelect,
}: {
  file: ProfileFileEntry;
  active: boolean;
  onSelect: (p: string) => void;
}) {
  const basename = file.path.split("/").pop() ?? file.path;
  const dot = basename.lastIndexOf(".");
  const stem = dot > 0 ? basename.slice(0, dot) : basename;
  const ext = dot > 0 ? basename.slice(dot + 1) : "";
  return (
    <button
      type="button"
      onClick={() => onSelect(file.path)}
      className={cn(
        "w-full text-left flex items-center justify-between gap-[6px] px-[12px] py-[5px] text-[12px] border-0 transition-colors cursor-pointer",
        active
          ? "bg-neutral-100 text-neutral-900"
          : "bg-transparent text-neutral-700 hover:bg-black/5 hover:text-neutral-900",
      )}
    >
      <span className="truncate">{stem}</span>
      {ext && (
        <span className="text-[10px] uppercase tracking-[0.06em] text-neutral-400 shrink-0">
          {ext}
        </span>
      )}
    </button>
  );
}

