// Titlebar.tsx
//
// Top bar: macOS traffic-light spacer (left), app name, center tab pill,
// settings button (right). Tauri's "Overlay" titleBarStyle keeps the
// traffic lights but hides the rest of the chrome — we leave a ~80px gap
// on the left so they don't overlap the app name.

import {
  FileInputIcon,
  SendIcon,
  BarChartIcon,
  SettingsIcon,
  type LucideIcon,
} from "lucide-react";

type TabItemProps = {
  icon: LucideIcon;
  label: string;
  active?: boolean;
};

function TabItem({ icon: Icon, label, active }: TabItemProps) {
  return (
    <button
      type="button"
      tabIndex={active ? 0 : -1}
      className={
        "h-[26px] px-[13px] rounded-[8px] inline-flex items-center gap-[6px] text-[13px] transition-colors " +
        (active
          ? "bg-white text-neutral-900 font-medium shadow-sm"
          : "text-neutral-400 hover:text-neutral-600")
      }
    >
      <Icon size={14} />
      {label}
    </button>
  );
}

export function Titlebar() {
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
        <div className="h-[32px] inline-flex items-center px-[3px]">
          <span className="text-[18px] font-medium text-neutral-700">
            Database Multitool
          </span>
        </div>
      </div>

      {/* Center — tab pill (wrapper is draggable; pill itself is not) */}
      <div data-tauri-drag-region className="flex items-center justify-center flex-1">
        <div className="flex items-center bg-neutral-200/70 rounded-[10px] p-[3px] gap-[1px]">
          <TabItem icon={FileInputIcon} label="Imports" active />
          <TabItem icon={SendIcon} label="Data Requests" />
          <TabItem icon={BarChartIcon} label="Reports" />
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
          className="w-[26px] h-[26px] rounded-[7px] inline-flex items-center justify-center text-neutral-500 border-0 bg-transparent hover:bg-black/5"
        >
          <SettingsIcon size={18} />
        </button>
      </div>
    </div>
  );
}
