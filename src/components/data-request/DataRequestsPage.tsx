// DataRequestsPage.tsx
//
// Data Requests tab. Stacks Library and Status as two separate cards
// floating on the App's neutral-100 background, separated by a
// transparent draggable strip that exposes the page bg as a visible
// gap. Each pane clamps to a minimum height so its header strip stays
// visible — the user can drag the divider all the way to either edge
// to collapse one pane down to just its header, then drag back.
//
// Layout state is tracked as a mode (`default | library-max | status-max
// | custom`) rather than a single ratio so that maximized panes pin the
// minimized one to exactly MIN_PANE_HEIGHT regardless of window size —
// otherwise growing the window would let the minimized pane reclaim
// proportional space. Drags use the `custom` mode with a stored ratio
// for proportional scaling.

import { useCallback, useRef, useState, type CSSProperties } from "react";
import { cn } from "../../lib/utils";
import { Panel } from "../shared/Panel";
import { Library } from "./Library";
import { Status } from "./Status";

// Matches the header-strip height inside Library/Status so a collapsed
// pane still shows its title and is grabbable.
const MIN_PANE_HEIGHT = 34;
const DIVIDER_HEIGHT = 8;
export const DEFAULT_LIBRARY_RATIO = 0.6;

const TRANSITION_MS = 150;

export type Mode = "default" | "custom" | "library-max" | "status-max";

// Layout state (`mode` + `customRatio`) is owned by App.tsx and passed in
// so it survives this page unmounting on tab navigation. Transient
// interaction state (`dragging`, `animating`) stays local.
interface DataRequestsPageProps {
  mode: Mode;
  setMode: (m: Mode) => void;
  customRatio: number;
  setCustomRatio: (r: number) => void;
}

export function DataRequestsPage({
  mode,
  setMode,
  customRatio,
  setCustomRatio,
}: DataRequestsPageProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);
  const [animating, setAnimating] = useState(false);
  const animTimeoutRef = useRef<number | null>(null);

  // The current Library-share of usable height as a ratio. Used to
  // anchor animations and drags coming out of any mode.
  const currentRatio = (usable: number): number => {
    switch (mode) {
      case "default":
        return DEFAULT_LIBRARY_RATIO;
      case "library-max":
        return (usable - MIN_PANE_HEIGHT) / usable;
      case "status-max":
        return MIN_PANE_HEIGHT / usable;
      case "custom":
        return customRatio;
    }
  };

  // Target Library-ratio for an intent mode at a given container size.
  const targetRatio = (intent: Mode, usable: number): number => {
    switch (intent) {
      case "default":
        return DEFAULT_LIBRARY_RATIO;
      case "library-max":
        return (usable - MIN_PANE_HEIGHT) / usable;
      case "status-max":
        return MIN_PANE_HEIGHT / usable;
      case "custom":
        return customRatio;
    }
  };

  const onDividerMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const usable = rect.height - DIVIDER_HEIGHT;
    if (usable <= 0) return;
    const minRatio = MIN_PANE_HEIGHT / usable;
    const maxRatio = 1 - minRatio;

    // Drags must be instant — kill any in-flight double-click animation.
    if (animTimeoutRef.current !== null) {
      clearTimeout(animTimeoutRef.current);
      animTimeoutRef.current = null;
    }
    setAnimating(false);

    // Anchor drag at the current visible ratio so we don't jump when
    // taking over from a maxed/default state.
    setCustomRatio(currentRatio(usable));
    setMode("custom");
    setDragging(true);

    const onMove = (ev: MouseEvent) => {
      const next = ev.clientY - rect.top;
      const r = Math.max(minRatio, Math.min(maxRatio, next / usable));
      setCustomRatio(r);
    };
    const onUp = () => {
      setDragging(false);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }, [mode, customRatio]);

  // Animate to a target intent mode. We pin to `custom` mode during the
  // transition (where flex-grow can interpolate), then snap to the
  // intent mode after — at the target ratio the two render to the same
  // pixel sizes, so the snap is invisible. Snapping to library-max or
  // status-max is what makes the layout survive window resize: the
  // minimized pane is pinned to MIN_PANE_HEIGHT via `flex: 0 0 34px`,
  // not a ratio that would scale with the window.
  const animateToMode = (intent: Mode) => {
    const container = containerRef.current;
    if (!container) return;
    const usable = container.getBoundingClientRect().height - DIVIDER_HEIGHT;
    if (usable <= 0) return;

    const startR = currentRatio(usable);
    const targetR = targetRatio(intent, usable);

    if (animTimeoutRef.current !== null) clearTimeout(animTimeoutRef.current);

    // Step 1: anchor in `custom` mode at startR with no transition.
    setAnimating(false);
    setMode("custom");
    setCustomRatio(startR);

    // Step 2: next frame enable transition and set the target ratio so
    // the browser sees a flex-grow change with `transition` active.
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        setAnimating(true);
        setCustomRatio(targetR);
      });
    });

    // Step 3: after the tween, snap to the intent mode so future window
    // resizes use the proper layout strategy.
    animTimeoutRef.current = window.setTimeout(() => {
      setAnimating(false);
      setMode(intent);
      animTimeoutRef.current = null;
    }, TRANSITION_MS + 50);
  };

  // Double-clicking a panel's title maximizes that panel — unless the
  // panel was already maximized, in which case it resets to the default
  // split. If the clicked panel was minimized, this swaps the layout.
  const handleTitleDoubleClick = (which: "library" | "status") => {
    const alreadyMaxed =
      (which === "library" && mode === "library-max") ||
      (which === "status" && mode === "status-max");
    if (alreadyMaxed) {
      animateToMode("default");
      return;
    }
    animateToMode(which === "library" ? "library-max" : "status-max");
  };

  const handleDividerDoubleClick = () => {
    animateToMode("default");
  };

  // Resolve styles from the mode. `library-max` / `status-max` pin the
  // minimized pane to MIN_PANE_HEIGHT so it stays minimized when the
  // window grows.
  const transition = animating
    ? `flex-grow ${TRANSITION_MS}ms ease-out, flex-basis ${TRANSITION_MS}ms ease-out`
    : undefined;
  const baseStyle: CSSProperties = {
    minHeight: `${MIN_PANE_HEIGHT}px`,
    transition,
  };
  let libraryStyle: CSSProperties;
  let statusStyle: CSSProperties;
  if (mode === "library-max") {
    libraryStyle = { ...baseStyle, flex: "1 1 0" };
    statusStyle = { ...baseStyle, flex: `0 0 ${MIN_PANE_HEIGHT}px` };
  } else if (mode === "status-max") {
    libraryStyle = { ...baseStyle, flex: `0 0 ${MIN_PANE_HEIGHT}px` };
    statusStyle = { ...baseStyle, flex: "1 1 0" };
  } else {
    const r = mode === "default" ? DEFAULT_LIBRARY_RATIO : customRatio;
    libraryStyle = { ...baseStyle, flex: `${r} 1 0` };
    statusStyle = { ...baseStyle, flex: `${1 - r} 1 0` };
  }

  return (
    <main
      ref={containerRef}
      className={cn(
        "h-full flex flex-col",
        dragging && "select-none cursor-row-resize",
      )}
    >
      <Panel style={libraryStyle}>
        <Library onTitleDoubleClick={() => handleTitleDoubleClick("library")} />
      </Panel>
      <div
        role="separator"
        aria-orientation="horizontal"
        onMouseDown={onDividerMouseDown}
        onDoubleClick={handleDividerDoubleClick}
        className="shrink-0 cursor-row-resize"
        style={{ height: `${DIVIDER_HEIGHT}px` }}
      />
      <Panel style={statusStyle}>
        <Status onTitleDoubleClick={() => handleTitleDoubleClick("status")} />
      </Panel>
    </main>
  );
}
