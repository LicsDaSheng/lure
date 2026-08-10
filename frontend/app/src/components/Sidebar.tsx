import { Plus, MessageSquare, Settings } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { SessionRow } from "@/lib/api";

export function Sidebar({
  sessions,
  activeKey,
  onSelect,
  onNew,
  onOpenSettings,
}: {
  sessions: SessionRow[];
  activeKey: string | null;
  onSelect: (key: string) => void;
  onNew: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <aside className="flex h-full w-[264px] shrink-0 flex-col border-r bg-sidebar text-sidebar-foreground">
      <div className="flex items-center justify-between px-4 py-3.5">
        <div className="flex items-center gap-2">
          <div className="flex size-7 items-center justify-center rounded-lg bg-accent text-accent-foreground">
            <span className="text-sm font-bold">L</span>
          </div>
          <span className="text-sm font-semibold tracking-tight">Lure</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="size-8"
          onClick={onOpenSettings}
          aria-label="设置"
        >
          <Settings className="size-4" />
        </Button>
      </div>

      <div className="px-3 pb-2">
        <Button
          variant="outline"
          className="w-full justify-start gap-2 rounded-lg"
          onClick={onNew}
        >
          <Plus className="size-4" />
          新建对话
        </Button>
      </div>

      <ScrollArea className="flex-1 px-2">
        <nav className="flex flex-col gap-0.5 py-1">
          {sessions.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-muted-foreground">
              还没有对话
            </p>
          ) : (
            sessions.map((s) => (
              <button
                key={s.key}
                onClick={() => onSelect(s.key)}
                className={cn(
                  "flex items-center gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors",
                  s.key === activeKey
                    ? "bg-accent/12 text-foreground ring-1 ring-accent/25"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <MessageSquare className="size-4 shrink-0 opacity-70" />
                <span className="truncate">
                  {s.title?.trim() || s.preview?.trim() || "新对话"}
                </span>
              </button>
            ))
          )}
        </nav>
      </ScrollArea>
    </aside>
  );
}
