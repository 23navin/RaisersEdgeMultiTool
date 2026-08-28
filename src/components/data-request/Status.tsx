// Status.tsx
//
// Bottom pane of the Data Requests page. Placeholder layout — header
// strip stays visible when the pane is collapsed via the resizer.
// Double-clicking the header is handled by the parent (maximize /
// reset).

type StatusProps = {
  onTitleDoubleClick?: () => void;
};

export function Status({ onTitleDoubleClick }: StatusProps) {
  return (
    <div className="flex flex-col h-full min-h-0">
      <div
        onDoubleClick={onTitleDoubleClick}
        className="px-[22px] h-[34px] flex items-center border-b border-neutral-200 shrink-0 select-none cursor-default"
      >
        <h2 className="text-[13px] font-medium text-neutral-900">Status</h2>
      </div>
      <div className="flex-1 overflow-y-auto px-[22px] py-[14px] text-[13px] text-neutral-500">
      </div>
    </div>
  );
}
