// PanelTransition.tsx
//
// Animates a tab panel into/out of view. The active panel renders in "enter"
// state during a transition and slides in from one side while scaling +
// fading up. The previously-active panel renders in "exit" state and slides
// out the same direction while scaling + fading down.
//
// `direction` is the way the panels move during the transition:
//   - 'left'  — moving toward a tab to the right of the previous one
//   - 'right' — moving toward a tab to the left of the previous one
//
// In both cases, the entering panel starts on the OPPOSITE side of the
// motion and travels through center; the exiting panel starts at center
// and travels off-screen in the direction of motion.

import { useEffect, useState, type ReactNode } from "react";

export const PANEL_TRANSITION_MS = 300;

const OFFSET_PERCENT = 5;
const OFFSET_SCALE = 0.95;

type Direction = "left" | "right";
type State = "enter" | "exit" | "idle";

type Props = {
  children: ReactNode;
  state: State;
  direction: Direction;
};

export function PanelTransition({ children, state, direction }: Props) {
  // "offset" — translated + scaled + transparent
  // "center" — neutral, fully visible
  const initialPhase: "offset" | "center" =
    state === "enter" ? "offset" : "center";
  const targetPhase: "offset" | "center" =
    state === "exit" ? "offset" : "center";

  const [phase, setPhase] = useState<"offset" | "center">(initialPhase);

  useEffect(() => {
    if (initialPhase === targetPhase) {
      setPhase(targetPhase);
      return;
    }
    // Double-RAF so the browser paints the initial "offset" state before we
    // flip to the target — otherwise the transition gets skipped.
    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => setPhase(targetPhase));
    });
    return () => {
      cancelAnimationFrame(raf1);
      cancelAnimationFrame(raf2);
    };
  }, [initialPhase, targetPhase]);

  let offsetSign = 0;
  if (phase === "offset") {
    if (state === "exit") {
      offsetSign = direction === "left" ? -1 : 1;
    } else if (state === "enter") {
      offsetSign = direction === "left" ? 1 : -1;
    }
  }

  const atOffset = phase === "offset";

  const style = {
    transform: `translate3d(${offsetSign * OFFSET_PERCENT}%, 0, 0) scale(${
      atOffset ? OFFSET_SCALE : 1
    })`,
    opacity: atOffset ? 0 : 1,
    transition: `transform ${PANEL_TRANSITION_MS}ms cubic-bezier(0.32, 0.72, 0.32, 1), opacity ${PANEL_TRANSITION_MS}ms cubic-bezier(0.32, 0.72, 0.32, 1)`,
    willChange: "transform, opacity",
  } as const;

  return (
    <div className="absolute inset-0" style={style}>
      {children}
    </div>
  );
}
