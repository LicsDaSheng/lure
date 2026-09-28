import { FolderIcon } from "lucide-react";

export function TaskHeader({
  title,
  onTitleChange,
}: {
  title: string;
  onTitleChange: (title: string) => void;
}) {
  return (
    <header
      aria-label="任务顶栏"
      className="flex h-14 shrink-0 items-center gap-2 border-b border-[#EEEEF0] bg-background px-4 md:px-5"
    >
      <FolderIcon
        aria-hidden="true"
        className="hidden size-4 shrink-0 text-muted-foreground sm:block"
      />
      <input
        aria-label="任务标题"
        className="min-w-0 flex-1 truncate bg-transparent text-sm font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        onChange={(event) => onTitleChange(event.target.value)}
        value={title}
      />
    </header>
  );
}
