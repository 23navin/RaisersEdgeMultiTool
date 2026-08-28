// SettingsPanel.tsx
//
// Floating settings card with a fullscreen backdrop.
// Backdrop is `fixed inset-0` so it blurs the entire window (titlebar +
// bg + body). The card itself is positioned `absolute inset-[28px]`
// within the body container, so it reads as a floating panel.
// Always mounted so it can animate in/out; visibility driven by `open`.
//
// This file is intentionally thin — tab contents live in ./settings/*.

import { useEffect, useState } from "react";
import { XIcon } from "lucide-react";
import { cn } from "../lib/utils";
import { ImportTab } from "./settings/imports/ImportTab";
import { GeneralTab } from "./settings/general/GeneralTab";

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
          "absolute inset-[0px] z-50 flex flex-col bg-white rounded-[12px] border border-neutral-200 shadow-2xl overflow-hidden origin-center",
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
