// SettingsPanel.tsx
//
// Floating settings card with a fullscreen backdrop.
// Backdrop is `fixed inset-0` so it blurs the entire window (titlebar +
// bg + body). The card itself is positioned `absolute inset-[28px]`
// within the body container, so it reads as a floating panel.
// Always mounted so it can animate in/out; visibility driven by `open`.

import { useEffect, useState } from "react";
import { SettingsIcon, XIcon } from "lucide-react";
import { cn } from "../lib/utils";

type Props = {
  open: boolean;
  onClose: () => void;
};

type Section = "general" | "profiles" | "about";

export function SettingsPanel({ open, onClose }: Props) {
  const [section, setSection] = useState<Section>("general");

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
            ? "opacity-100 backdrop-blur-md bg-black/5 pointer-events-auto"
            : "opacity-0 backdrop-blur-0 bg-transparent pointer-events-none",
        )}
      />

      {/* Card — inset within the body container so it reads as a floating panel */}
      <div
        aria-hidden={!open}
        className={cn(
          "absolute inset-[28px] z-50 flex bg-white rounded-[12px] border border-neutral-200 shadow-2xl overflow-hidden origin-center",
          "transition-[opacity,filter,transform] duration-200 ease-out",
          open
            ? "opacity-100 blur-0 scale-100 pointer-events-auto"
            : "opacity-0 blur-md scale-[1.04] pointer-events-none",
        )}
      >
      {/* Left — section nav */}
      <div className="w-[220px] shrink-0 border-r border-neutral-200 bg-neutral-50/60 flex flex-col">
        <div className="h-[56px] flex items-center gap-[8px] px-[16px] border-b border-neutral-200">
          <SettingsIcon size={16} className="text-neutral-500" />
          <span className="text-[14px] font-medium text-neutral-800">
            Settings
          </span>
        </div>
        <nav className="flex flex-col gap-[2px] p-[8px]">
          <SectionItem
            label="General"
            active={section === "general"}
            onClick={() => setSection("general")}
          />
          <SectionItem
            label="Profiles"
            active={section === "profiles"}
            onClick={() => setSection("profiles")}
          />
          <SectionItem
            label="About"
            active={section === "about"}
            onClick={() => setSection("about")}
          />
        </nav>
      </div>

      {/* Right — section body */}
      <div className="flex-1 flex flex-col overflow-hidden">
        <div className="h-[56px] shrink-0 flex items-center justify-between px-[20px] border-b border-neutral-200">
          <h2 className="text-[15px] font-semibold text-neutral-900">
            {section === "general" && "General"}
            {section === "profiles" && "Profiles"}
            {section === "about" && "About"}
          </h2>
          <button
            type="button"
            aria-label="Close settings"
            onClick={onClose}
            className="w-[28px] h-[28px] rounded-[7px] inline-flex items-center justify-center text-neutral-500 border-0 bg-transparent hover:bg-black/5"
          >
            <XIcon size={16} />
          </button>
        </div>

        <div className="flex-1 overflow-auto p-[24px]">
          {section === "general" && <GeneralSection />}
          {section === "profiles" && <ProfilesSection />}
          {section === "about" && <AboutSection />}
        </div>
      </div>
      </div>
    </>
  );
}

function SectionItem({
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
      onClick={onClick}
      className={cn(
        "text-left px-[12px] py-[7px] rounded-[7px] text-[13px] transition-colors border-0",
        active
          ? "bg-white text-neutral-900 shadow-sm font-medium"
          : "bg-transparent text-neutral-600 hover:bg-black/5",
      )}
    >
      {label}
    </button>
  );
}

function Row({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-[16px] py-[14px] border-b border-neutral-100 last:border-b-0">
      <div className="flex flex-col">
        <span className="text-[13px] font-medium text-neutral-800">{label}</span>
        {description && (
          <span className="text-[12px] text-neutral-500 mt-[2px]">
            {description}
          </span>
        )}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function GeneralSection() {
  return (
    <div className="max-w-[640px]">
      <Row
        label="Theme"
        description="Match the system appearance or pick a fixed theme."
      >
        <select className="text-[13px] px-[10px] py-[5px] rounded-[7px] border border-neutral-200 bg-white">
          <option>System</option>
          <option>Light</option>
          <option>Dark</option>
        </select>
      </Row>
      <Row
        label="Show notices inline"
        description="Display validation notices alongside file uploads."
      >
        <input type="checkbox" defaultChecked className="h-[16px] w-[16px]" />
      </Row>
      <Row
        label="Confirm before reset"
        description="Ask before clearing the current workflow."
      >
        <input type="checkbox" className="h-[16px] w-[16px]" />
      </Row>
    </div>
  );
}

function ProfilesSection() {
  return (
    <div className="max-w-[640px]">
      <Row
        label="User profiles folder"
        description="Drop .import bundles here to install them."
      >
        <button
          type="button"
          className="text-[13px] px-[12px] py-[6px] rounded-[7px] border border-neutral-200 bg-white hover:bg-neutral-50"
        >
          Open folder
        </button>
      </Row>
      <Row
        label="Install profile"
        description="Pick a .import file to copy into your profiles folder."
      >
        <button
          type="button"
          className="text-[13px] px-[12px] py-[6px] rounded-[7px] border border-neutral-200 bg-white hover:bg-neutral-50"
        >
          Choose file…
        </button>
      </Row>
      <Row
        label="Reload profile list"
        description="Re-scan the profiles folder."
      >
        <button
          type="button"
          className="text-[13px] px-[12px] py-[6px] rounded-[7px] border border-neutral-200 bg-white hover:bg-neutral-50"
        >
          Reload
        </button>
      </Row>
    </div>
  );
}

function AboutSection() {
  return (
    <div className="max-w-[640px] flex flex-col gap-[14px]">
      <div>
        <div className="text-[15px] font-semibold text-neutral-900">
          Database Multitool
        </div>
        <div className="text-[12px] text-neutral-500 mt-[2px]">
          Version 0.1.0
        </div>
      </div>
      <p className="text-[13px] text-neutral-600 leading-[1.5]">
        Transforms vendor-supplied files into the target database&apos;s import
        format. Profile bundles define each vendor&apos;s rules; drop new
        <code className="mx-[4px] px-[5px] py-[1px] rounded-[4px] bg-neutral-100 text-[12px]">
          .import
        </code>
        files into the profiles folder to add support without recompiling.
      </p>
    </div>
  );
}
