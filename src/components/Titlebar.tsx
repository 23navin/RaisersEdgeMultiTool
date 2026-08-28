// Titlebar.tsx
//
// Top bar: macOS traffic-light spacer (left), app name, center tab pill,
// settings button (right). Tauri's "Overlay" titleBarStyle keeps the
// traffic lights but hides the rest of the chrome — we leave a ~80px gap
// on the left so they don't overlap the app name.

import { useRef } from "react";
import {
  FileInputIcon,
  SendIcon,
  BarChartIcon,
  SettingsIcon,
  type LucideIcon,
} from "lucide-react";

// Distance (px) the cursor can move between mousedown and click before we
// treat the gesture as a window drag and suppress the click.
const DRAG_CLICK_THRESHOLD = 4;

export type TopTab = "imports" | "data-requests" | "reports";

type TabItemProps = {
  icon: LucideIcon;
  label: string;
  active?: boolean;
  onClick?: () => void;
};

function TabItem({ icon: Icon, label, active, onClick }: TabItemProps) {
  const downPos = useRef<{ x: number; y: number } | null>(null);

  return (
    <button
      type="button"
      tabIndex={active ? 0 : -1}
      data-tauri-drag-region
      onMouseDown={(e) => {
        // Use screen coords — clientX/Y move with the window during a Tauri
        // drag, so they'd register zero movement even after a long drag.
        downPos.current = { x: e.screenX, y: e.screenY };
      }}
      onClick={(e) => {
        const start = downPos.current;
        downPos.current = null;
        if (start) {
          const dx = e.screenX - start.x;
          const dy = e.screenY - start.y;
          if (Math.hypot(dx, dy) > DRAG_CLICK_THRESHOLD) return;
        }
        onClick?.();
      }}
      className={
        "h-[26px] px-[13px] rounded-[8px] inline-flex items-center gap-[6px] text-[13px] transition-colors " +
        (active
          ? "bg-white text-neutral-900 shadow-sm"
          : "text-neutral-400 hover:text-neutral-600")
      }
    >
      <Icon size={14} data-tauri-drag-region />
      <span data-tauri-drag-region>{label}</span>
    </button>
  );
}

type TitlebarProps = {
  activeTab: TopTab;
  onTabChange: (tab: TopTab) => void;
  onOpenSettings: () => void;
};

export function Titlebar({ activeTab, onTabChange, onOpenSettings }: TitlebarProps) {
  const settingsDownPos = useRef<{ x: number; y: number } | null>(null);

  return (
    <div
      data-tauri-drag-region
      className="h-[48px] flex items-center justify-between pr-[16px] select-none"
    >
      {/* Left — traffic-light spacer + app name.
          Wrapped in an h-[32px] block to match the tab pill's height so
          both rows sit on the same horizontal baseline. */}
      <div
        data-tauri-drag-region
        className="min-w-[220px] flex items-center gap-[9px] pl-[90px]"
      >
        <div
          data-tauri-drag-region
          className="h-[32px] inline-flex items-center px-[3px]"
        >
          <span
            data-tauri-drag-region
            className="text-[18px] font-medium text-neutral-700"
          >
            Database Multitool
          </span>
        </div>
      </div>

      {/* Center — tab pill (wrapper is draggable; pill itself is not) */}
      <div data-tauri-drag-region className="flex items-center justify-center flex-1">
        <div
          data-tauri-drag-region
          className="flex items-center bg-neutral-200/70 rounded-[10px] p-[3px] gap-[1px]"
        >
          <TabItem
            icon={FileInputIcon}
            label="Imports"
            active={activeTab === "imports"}
            onClick={() => onTabChange("imports")}
          />
          <TabItem
            icon={SendIcon}
            label="Data Requests"
            active={activeTab === "data-requests"}
            onClick={() => onTabChange("data-requests")}
          />
          <TabItem
            icon={BarChartIcon}
            label="Reports"
            active={activeTab === "reports"}
            onClick={() => onTabChange("reports")}
          />
        </div>
      </div>

      {/* Right — settings */}
      <div
        data-tauri-drag-region
        className="min-w-[180px] flex items-center justify-end"
      >
        <button
          type="button"
          aria-label="Settings"
          data-tauri-drag-region
          onMouseDown={(e) => {
            settingsDownPos.current = { x: e.screenX, y: e.screenY };
          }}
          onClick={(e) => {
            const start = settingsDownPos.current;
            settingsDownPos.current = null;
            if (start) {
              const dx = e.screenX - start.x;
              const dy = e.screenY - start.y;
              if (Math.hypot(dx, dy) > DRAG_CLICK_THRESHOLD) return;
            }
            onOpenSettings();
          }}
          className="w-[26px] h-[26px] rounded-[7px] inline-flex items-center justify-center text-neutral-500 border-0 bg-transparent hover:bg-black/5"
        >
          <SettingsIcon size={18} data-tauri-drag-region />
        </button>
      </div>
    </div>
  );
}
