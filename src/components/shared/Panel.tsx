// Panel.tsx
//
// Standard white card used across Imports, Data Requests, and Reports.
// Internal overflow-hidden clips children to the rounded corners. The
// panel's own shadow renders outside its box — surrounding containers
// must give it clearance (no overflow-hidden over the shadow area).

import type { CSSProperties, ReactNode } from "react";
import { cn } from "../../lib/utils";

type PanelProps = {
  children: ReactNode;
  className?: string;
  style?: CSSProperties;
};

export function Panel({ children, className, style }: PanelProps) {
  return (
    <div
      style={style}
      className={cn(
        "bg-white rounded-xl border border-neutral-200 shadow-md overflow-hidden",
        className,
      )}
    >
      {children}
    </div>
  );
}
