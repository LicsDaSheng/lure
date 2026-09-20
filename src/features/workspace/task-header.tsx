import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { DownloadIcon, FolderIcon, MoreHorizontalIcon, MenuIcon, UnplugIcon } from "lucide-react";

export function TaskHeader({
  title,
  canDisconnect,
  onTitleChange,
  onChooseDirectory,
  onDisconnect,
  onExport,
  onOpenNavigation,
}: {
  title: string;
  canDisconnect: boolean;
  onTitleChange: (title: string) => void;
  onChooseDirectory: () => void;
  onDisconnect: () => void;
  onExport: () => void;
  onOpenNavigation: () => void;
}) {
  return (
    <header
      aria-label="任务顶栏"
      className="flex h-14 shrink-0 items-center gap-2 border-b border-border/60 px-4 md:px-5"
    >
      <Button
        aria-label="打开任务导航"
        className="md:hidden"
        onClick={onOpenNavigation}
        size="icon"
        type="button"
        variant="ghost"
      >
        <MenuIcon />
      </Button>

      <FolderIcon aria-hidden="true" className="hidden size-4 shrink-0 text-muted-foreground sm:block" />
      <input
        aria-label="任务标题"
        className="min-w-0 flex-1 truncate bg-transparent text-sm font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
        onChange={(event) => onTitleChange(event.target.value)}
        value={title}
      />

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button aria-label="更多任务操作" size="icon" type="button" variant="ghost">
            <MoreHorizontalIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onSelect={onChooseDirectory}>
            <FolderIcon />
            更换工作目录
          </DropdownMenuItem>
          {canDisconnect && (
            <>
              <DropdownMenuSeparator />
              <DropdownMenuItem onSelect={onDisconnect}>
                <UnplugIcon />
                断开 Pi
              </DropdownMenuItem>
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      <Button
        aria-label="导出记录"
        onClick={onExport}
        size="icon"
        type="button"
        variant="ghost"
      >
        <DownloadIcon />
      </Button>
    </header>
  );
}