// CodeMirrorEditor.tsx
//
// Wraps @uiw/react-codemirror with the bits the profile editor needs:
//   - Per-file language: YAML / Markdown / SQL
//   - Tab inserts two spaces (`indentUnit` of two spaces + Tab keybinding)
//   - cmd/ctrl-click on a navigable token jumps across files:
//       yaml `- label: X` (under steps:)        → instructions.md `<!-- label: X -->`
//       yaml `sql: foo.sql`                      → sql/foo.sql
//       md   `<!-- label: X -->`                 → structure.yaml step
//       sql  `{{input:X}}` / `{{output:X}}`      → input/output def in yaml
//
// The anchor map is built once per profile load and passed in. The click
// handler looks at the line text under the cursor, finds the navigable
// token at that column, and emits a NavTarget for the parent to handle.

import { useEffect, useMemo, useRef } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { yaml } from "@codemirror/lang-yaml";
import { sql } from "@codemirror/lang-sql";
import { markdown } from "@codemirror/lang-markdown";
import {
  Decoration,
  EditorView,
  keymap,
  type DecorationSet,
} from "@codemirror/view";
import {
  StateEffect,
  StateField,
  type EditorState,
  type Range,
} from "@codemirror/state";
import { indentUnit } from "@codemirror/language";
import { indentWithTab } from "@codemirror/commands";
import type { ProfileFileEntry, IssueLocation, Severity } from "../types";

// ── Anchor model ─────────────────────────────────────────────────────────────

export type NavTarget = { path: string; line: number };

export type AnchorMap = {
  yamlStepLabels: Map<string, number>;   // step label  → 1-indexed line in structure.yaml
  yamlInputLabels: Map<string, number>;  // input label → line
  yamlOutputLabels: Map<string, number>; // output label → line
  mdStepLabels: Map<string, number>;     // step label  → 1-indexed line in instructions.md
};

const EMPTY_ANCHORS: AnchorMap = {
  yamlStepLabels: new Map(),
  yamlInputLabels: new Map(),
  yamlOutputLabels: new Map(),
  mdStepLabels: new Map(),
};

// Strip a single layer of YAML quoting from a scalar value.
function unquote(s: string): string {
  const trimmed = s.trim();
  const m = trimmed.match(/^"(.*)"$/) ?? trimmed.match(/^'(.*)'$/);
  return m ? m[1] : trimmed;
}

// Walk structure.yaml line-by-line, tracking the current top-level section
// (`steps:` / `inputs:` / `outputs:`) so we can attribute each `- label: X`
// line to the right bucket. Anything indented deeper than the section item
// (e.g. validation rows nested under an input) is ignored.
export function buildAnchorMap(files: ProfileFileEntry[]): AnchorMap {
  const yamlFile = files.find((f) => f.path === "structure.yaml");
  const mdFile = files.find((f) => f.path === "instructions.md");

  const yamlStepLabels = new Map<string, number>();
  const yamlInputLabels = new Map<string, number>();
  const yamlOutputLabels = new Map<string, number>();

  if (yamlFile) {
    let section: "steps" | "inputs" | "outputs" | null = null;
    const lines = yamlFile.content.split("\n");
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      if (/^[a-zA-Z_]/.test(line)) {
        if (line.startsWith("steps:")) section = "steps";
        else if (line.startsWith("inputs:")) section = "inputs";
        else if (line.startsWith("outputs:")) section = "outputs";
        else section = null;
        continue;
      }
      // Top-level item under the section: `  - label: VALUE`
      const m = line.match(/^  - label:\s*(.+?)\s*$/);
      if (!m || !section) continue;
      const label = unquote(m[1]);
      const lineNum = i + 1;
      if (section === "steps" && !yamlStepLabels.has(label)) {
        yamlStepLabels.set(label, lineNum);
      } else if (section === "inputs" && !yamlInputLabels.has(label)) {
        yamlInputLabels.set(label, lineNum);
      } else if (section === "outputs" && !yamlOutputLabels.has(label)) {
        yamlOutputLabels.set(label, lineNum);
      }
    }
  }

  const mdStepLabels = new Map<string, number>();
  if (mdFile) {
    mdFile.content.split("\n").forEach((line, i) => {
      const m = line.match(/<!--\s*label:\s*(.+?)\s*-->/);
      if (m) mdStepLabels.set(m[1].trim(), i + 1);
    });
  }

  return { yamlStepLabels, yamlInputLabels, yamlOutputLabels, mdStepLabels };
}

// ── Issue → editor location ──────────────────────────────────────────────────
// Converts a semantic IssueLocation from the backend validator into an
// editor-addressable {path, line} pair, using the anchor map to resolve
// label-based references to line numbers.

export function issueToFileLine(
  loc: IssueLocation | null | undefined,
  anchors: AnchorMap,
): { path: string; line: number } | null {
  if (!loc) return null;
  switch (loc.kind) {
    case "yaml_step": {
      const line = anchors.yamlStepLabels.get(loc.label);
      return line ? { path: "structure.yaml", line } : { path: "structure.yaml", line: 1 };
    }
    case "yaml_input": {
      const line = anchors.yamlInputLabels.get(loc.label);
      return line ? { path: "structure.yaml", line } : { path: "structure.yaml", line: 1 };
    }
    case "yaml_output": {
      const line = anchors.yamlOutputLabels.get(loc.label);
      return line ? { path: "structure.yaml", line } : { path: "structure.yaml", line: 1 };
    }
    case "yaml_line":
      return { path: "structure.yaml", line: loc.line };
    case "md_anchor": {
      const line = anchors.mdStepLabels.get(loc.label);
      return line ? { path: "instructions.md", line } : { path: "instructions.md", line: 1 };
    }
    case "md_line":
      return { path: "instructions.md", line: loc.line };
    case "sql":
      return { path: loc.path, line: loc.line ?? 1 };
    case "file":
      return { path: loc.path, line: 1 };
  }
}

// ── File-kind detection ──────────────────────────────────────────────────────

type FileKind = "yaml" | "md" | "sql" | "unknown";

function fileKind(path: string): FileKind {
  if (path.endsWith(".yaml") || path.endsWith(".yml")) return "yaml";
  if (path.endsWith(".md")) return "md";
  if (path.endsWith(".sql")) return "sql";
  return "unknown";
}

// ── Click resolution ─────────────────────────────────────────────────────────
// Given the line under the cursor and the click column, returns a target if
// the click lands inside a known navigable token, otherwise null.

function targetAt(
  kind: FileKind,
  lineText: string,
  col: number,
  anchors: AnchorMap,
): NavTarget | null {
  if (kind === "yaml") {
    // sql: <filename>
    const sqlRef = lineText.match(/(sql:\s*)([\w./-]+)/);
    if (sqlRef && sqlRef.index != null) {
      const start = sqlRef.index + sqlRef[1].length;
      const end = start + sqlRef[2].length;
      if (col >= start && col <= end) {
        const filename = sqlRef[2];
        const path = filename.includes("/") ? filename : `sql/${filename}`;
        return { path, line: 1 };
      }
    }
    // - label: <value>  → md anchor
    const labelRef = lineText.match(/(-\s*label:\s*"?)([^"\n]+?)("?\s*$)/);
    if (labelRef && labelRef.index != null) {
      const start = labelRef.index + labelRef[1].length;
      const end = start + labelRef[2].length;
      if (col >= start && col <= end) {
        const label = labelRef[2].trim();
        const line = anchors.mdStepLabels.get(label);
        if (line != null) return { path: "instructions.md", line };
      }
    }
  }

  if (kind === "md") {
    const m = lineText.match(/(<!--\s*label:\s*)(.+?)(\s*-->)/);
    if (m && m.index != null) {
      const start = m.index + m[1].length;
      const end = start + m[2].length;
      if (col >= start && col <= end) {
        const label = m[2].trim();
        const line = anchors.yamlStepLabels.get(label);
        if (line != null) return { path: "structure.yaml", line };
      }
    }
  }

  if (kind === "sql") {
    const regex = /\{\{(input|output):([^}]+)\}\}/g;
    let m: RegExpExecArray | null;
    while ((m = regex.exec(lineText)) !== null) {
      const start = m.index;
      const end = start + m[0].length;
      if (col >= start && col <= end) {
        const which = m[1];
        const name = m[2].trim();
        const line = which === "input"
          ? anchors.yamlInputLabels.get(name)
          : anchors.yamlOutputLabels.get(name);
        if (line != null) return { path: "structure.yaml", line };
      }
    }
  }

  return null;
}

// ── Issue squiggle decorations ──────────────────────────────────────────────
// IDE-style wavy underline for problem lines. Locations carry line numbers
// only (no column ranges), so we underline the trimmed content of each line.

export type EditorIssue = {
  line: number;        // 1-indexed line in the current file
  severity: Severity;
  message: string;     // shown as a native `title` tooltip on hover
};

const SEVERITY_RANK: Record<Severity, number> = { info: 0, warning: 1, error: 2 };

const setIssuesEffect = StateEffect.define<EditorIssue[]>();

function buildIssueDecorations(
  state: EditorState,
  issues: EditorIssue[],
): DecorationSet {
  if (issues.length === 0) return Decoration.none;

  // Coalesce multiple issues on the same line: take the worst severity for
  // the visual, concatenate messages for the tooltip.
  const byLine = new Map<number, EditorIssue[]>();
  for (const issue of issues) {
    const list = byLine.get(issue.line);
    if (list) list.push(issue);
    else byLine.set(issue.line, [issue]);
  }

  const ranges: Range<Decoration>[] = [];
  for (const [lineNum, group] of byLine) {
    if (lineNum < 1 || lineNum > state.doc.lines) continue;
    const line = state.doc.line(lineNum);
    const text = line.text;
    const leftPad = text.length - text.trimStart().length;
    const rightTrim = text.trimEnd().length;
    if (rightTrim <= leftPad) continue; // blank line
    const from = line.from + leftPad;
    const to = line.from + rightTrim;

    const worst = group.reduce((a, b) =>
      SEVERITY_RANK[b.severity] > SEVERITY_RANK[a.severity] ? b : a,
    );
    const title = group.map((i) => i.message).join("\n");

    ranges.push(
      Decoration.mark({
        class: `cm-issue cm-issue-${worst.severity}`,
        attributes: { title },
      }).range(from, to),
    );
  }
  ranges.sort((a, b) => a.from - b.from);
  return Decoration.set(ranges);
}

// Factory: produces a fresh field whose `create()` reads the current issues
// off the supplied ref. The editor is remounted (via `key`) when the active
// file changes, so each mount gets a field initialised with the right issues
// — guaranteeing decorations are present on the first render after a
// cross-file navigate (no race with `useEffect` dispatch / ref population).
function makeIssueField(issuesRef: { current: EditorIssue[] }) {
  return StateField.define<DecorationSet>({
    create: (state) => buildIssueDecorations(state, issuesRef.current),
    update(deco, tr) {
      let next = deco.map(tr.changes);
      for (const e of tr.effects) {
        if (e.is(setIssuesEffect)) {
          next = buildIssueDecorations(tr.state, e.value);
        }
      }
      return next;
    },
    provide: (f) => EditorView.decorations.from(f),
  });
}

const issueTheme = EditorView.baseTheme({
  ".cm-issue-error": {
    textDecoration: "underline wavy #dc2626",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-issue-warning": {
    textDecoration: "underline wavy #d97706",
    textDecorationSkipInk: "none",
    textUnderlineOffset: "3px",
  },
  ".cm-issue-info": {
    textDecoration: "underline dotted #2563eb",
    textUnderlineOffset: "3px",
  },
});

// ── React component ─────────────────────────────────────────────────────────

type Props = {
  path: string;
  value: string;
  onChange: (next: string) => void;
  readOnly?: boolean;
  anchors?: AnchorMap;
  onNavigate?: (target: NavTarget) => void;
  // Bumping `scrollToLine.nonce` re-triggers a scroll to `scrollToLine.line`,
  // even if the line number itself didn't change. Used by the parent after
  // a cmd-click handoff.
  scrollToLine?: { line: number; nonce: number } | null;
  // Issues affecting the currently-displayed file. Drives squiggle decorations.
  issues?: EditorIssue[];
};

export function CodeMirrorEditor({
  path,
  value,
  onChange,
  readOnly = false,
  anchors = EMPTY_ANCHORS,
  onNavigate,
  scrollToLine,
  issues,
}: Props) {
  const kind = fileKind(path);
  const ref = useRef<ReactCodeMirrorRef>(null);

  // Keep the latest anchors / callback reachable from the click handler
  // without rebuilding the extension array (which would recreate the editor).
  const anchorsRef = useRef(anchors);
  const onNavigateRef = useRef(onNavigate);
  useEffect(() => { anchorsRef.current = anchors; }, [anchors]);
  useEffect(() => { onNavigateRef.current = onNavigate; }, [onNavigate]);

  // Mutable mirror of the latest issues so the field factory can seed the
  // initial decoration set on mount (synchronous, no dispatch round-trip).
  const issuesRef = useRef<EditorIssue[]>(issues ?? []);
  issuesRef.current = issues ?? [];

  const extensions = useMemo(() => {
    const langExt = kind === "yaml" ? [yaml()]
      : kind === "md" ? [markdown()]
      : kind === "sql" ? [sql()]
      : [];

    const clickHandler = EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!event.metaKey && !event.ctrlKey) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        const line = view.state.doc.lineAt(pos);
        const col = pos - line.from;
        const target = targetAt(kind, line.text, col, anchorsRef.current);
        if (!target) return false;
        event.preventDefault();
        onNavigateRef.current?.(target);
        return true;
      },
    });

    return [
      ...langExt,
      indentUnit.of("  "),
      EditorView.lineWrapping,
      keymap.of([indentWithTab]),
      clickHandler,
      makeIssueField(issuesRef),
      issueTheme,
    ];
  }, [kind]);

  // Push the latest issues into the editor as a state effect whenever the
  // prop changes. Kept out of `extensions` so the editor isn't rebuilt on
  // every validate.
  useEffect(() => {
    const view = ref.current?.view;
    if (!view) return;
    view.dispatch({ effects: setIssuesEffect.of(issues ?? []) });
  }, [issues]);

  // Scroll to a target line after the parent switches us to a new file or
  // jumps to a line within the current file.
  useEffect(() => {
    if (!scrollToLine) return;
    const view = ref.current?.view;
    if (!view) return;
    const total = view.state.doc.lines;
    const lineNum = Math.min(Math.max(scrollToLine.line, 1), total);
    const line = view.state.doc.line(lineNum);
    view.dispatch({
      selection: { anchor: line.from },
      effects: EditorView.scrollIntoView(line.from, { y: "center" }),
    });
    view.focus();
  }, [scrollToLine?.nonce, scrollToLine?.line]);

  return (
    <CodeMirror
      ref={ref}
      value={value}
      onChange={onChange}
      readOnly={readOnly}
      height="100%"
      basicSetup={{
        lineNumbers: true,
        foldGutter: false,
        highlightActiveLine: !readOnly,
        autocompletion: false,
      }}
      extensions={extensions}
      className="flex-1 min-h-0 text-[12px]"
    />
  );
}
