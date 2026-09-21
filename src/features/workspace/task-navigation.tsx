import { Button } from "@/components/ui/button";
import type { ConnectionPhase } from "@/features/pi-connection";
import { BotIcon, PlusIcon, SettingsIcon, XIcon } from "lucide-react";

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
      className={`fixed inset-y-0 left-0 z-40 flex w-64 shrink-0 flex-col border-r border-border/60 bg-card p-3 transition-transform duration-200 md:static md:translate-x-0 ${
        open ? "translate-x-0" : "-translate-x-full"
      }`}
    >
      <div className="flex min-h-12 items-center gap-2 px-2">
        <div className="grid size-8 place-items-center rounded-lg bg-primary text-primary-foreground">
          <BotIcon aria-hidden="true" className="size-4" />
        </div>
        <div className="min-w-0">
          <p className="font-semibold tracking-tight">Lure</p>
          <p className="text-xs text-muted-foreground">个人智能体工作台</p>
        </div>
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

      <Button className="mt-4 w-full justify-start" onClick={onNewTask} type="button">
        <PlusIcon />
        新建任务
      </Button>

      <div className="mt-5 min-h-0 flex-1 overflow-y-auto px-1">
        <section aria-labelledby="today-tasks">
          <h2 className="px-2 text-xs font-medium text-muted-foreground" id="today-tasks">
            今天
          </h2>
          {hasTask ? (
            <button
              aria-current="page"
              className="mt-1 flex w-full items-center gap-2 rounded-lg bg-accent px-2.5 py-2 text-left text-sm font-medium text-accent-foreground"
              onClick={onSelectTask}
              type="button"
            >
              <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-[var(--pi-accent)]" />
              <span className="min-w-0 flex-1 truncate">{taskTitle}</span>
              <span className="text-[11px] font-normal text-muted-foreground">
                {phaseLabels[phase]}
              </span>
            </button>
          ) : (
            <p className="px-2 py-2 text-xs text-muted-foreground">暂无任务</p>
          )}
        </section>

      </div>

      <Button className="mt-3 w-full justify-start" type="button" variant="ghost">
        <SettingsIcon />
        设置
      </Button>
    </nav>
  );
}