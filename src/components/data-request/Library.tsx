// Library.tsx
//
// Top pane of the Data Requests page. Placeholder layout — header strip
// stays visible when the pane is collapsed via the resizer so the user
// can always grab the divider to expand it again. Double-clicking the
// header is handled by the parent (maximize / reset).

type LibraryProps = {
  onTitleDoubleClick?: () => void;
};

export function Library({ onTitleDoubleClick }: LibraryProps) {
  return (
    <div className="flex flex-col h-full min-h-0 bg-white rounded-xl border border-neutral-200 shadow-md overflow-hidden">
      <div
        onDoubleClick={onTitleDoubleClick}
        className="px-[22px] h-[34px] flex items-center border-b border-neutral-200 shrink-0 select-none cursor-default"
      >
        <h2 className="text-[13px] font-medium text-neutral-900">Library</h2>
      </div>
      <div className="flex-1 overflow-y-auto px-[22px] py-[14px] text-[13px] text-neutral-500">
      </div>
    </div>
  );
}
