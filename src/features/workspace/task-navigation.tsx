import { Button } from "@/components/ui/button";
import type { ConnectionPhase } from "@/features/pi-connection";
import { ChevronDownIcon, PlusIcon, SettingsIcon, XIcon } from "lucide-react";

const phaseLabels: Record<ConnectionPhase, string> = {
  disconnected: "未连接",
  connecting: "正在连接",
  ready: "已连接",
  running: "执行中",
  failed: "连接失败",
};

export function TaskNavigation({
  open,
  taskTitle,
  phase,
  hasTask,
  onNewTask,
  onSelectTask,
  onClose,
}: {
  open: boolean;
  taskTitle: string;
  phase: ConnectionPhase;
  hasTask: boolean;
  onNewTask: () => void;
  onSelectTask: () => void;
  onClose: () => void;
}) {
  return (
    <nav
      aria-label="任务导航"
      className={`fixed inset-y-0 left-0 z-40 flex w-64 shrink-0 flex-col border-r border-[#E8E8EB] bg-[#F6F6F8] transition-transform duration-200 md:static md:translate-x-0 ${
        open ? "translate-x-0" : "-translate-x-full"
      }`}
    >
      <div className="flex h-14 items-center gap-1 px-5">
        <span className="text-sm font-semibold tracking-tight">Lure</span>
        <ChevronDownIcon aria-hidden="true" className="size-3.5 text-muted-foreground" />
        <Button
          aria-expanded={open}
          aria-label="关闭任务导航"
          className="ml-auto md:hidden"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <XIcon />
        </Button>
      </div>

      <div className="border-b border-[#E8E8EB]">
        <Button
          className="h-11 w-full justify-start rounded-none px-5"
          onClick={onNewTask}
          type="button"
          variant="ghost"
        >
          <PlusIcon />
          新建任务
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-5">
        <section aria-labelledby="today-tasks" className="mt-6">
          <h2 className="text-[13px] font-medium text-muted-foreground" id="today-tasks">
            今天
          </h2>
          {hasTask ? (
            <button
              aria-current="page"
              className="mt-1 flex h-10 w-full items-center gap-2 rounded-lg bg-[#E9E9ED] px-3 text-left text-sm font-medium text-accent-foreground"
              onClick={onSelectTask}
              type="button"
            >
              <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-[var(--pi-accent)]" />
              <span className="min-w-0 flex-1 truncate">{taskTitle}</span>
              <span className="text-[10px] font-normal text-muted-foreground">
                {phaseLabels[phase]}
              </span>
            </button>
          ) : (
            <p className="py-2 text-xs text-muted-foreground">暂无任务</p>
          )}
        </section>

      </div>

      <Button className="mb-3 h-11 w-full justify-start px-5" type="button" variant="ghost">
        <SettingsIcon />
        设置
      </Button>
    </nav>
  );
}