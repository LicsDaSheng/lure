import { Button } from "@/components/ui/button";
import type {
  ConnectionPhase,
  ProjectDescriptor,
  RecentConversation,
} from "@/features/pi-connection";
import {
  ChevronDownIcon,
  FolderIcon,
  MoreHorizontalIcon,
  PlusIcon,
  SettingsIcon,
  SquarePenIcon,
  XIcon,
} from "lucide-react";

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
  onNewProject,
  onNewProjectTask,
  projects,
  recentConversations,
  activeDirectory,
  activeSessionId,
  defaultWorkspace,
  disabled,
  onSelectTask,
  onClose,
}: {
  open: boolean;
  taskTitle: string;
  phase: ConnectionPhase;
  hasTask: boolean;
  onNewTask: () => void;
  onNewProject: () => void;
  onNewProjectTask: (directory: string) => void;
  projects: ProjectDescriptor[];
  recentConversations: RecentConversation[];
  activeDirectory: string | null;
  activeSessionId: string | null;
  defaultWorkspace: string | null;
  disabled: boolean;
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
          disabled={disabled}
          type="button"
          variant="ghost"
        >
          <PlusIcon />
          新建任务
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-3">
        <section aria-labelledby="recent-heading" className="mt-5">
          <div className="flex h-9 items-center px-2">
            <h2 className="text-[13px] font-medium text-muted-foreground" id="recent-heading">
              最近
            </h2>
          </div>
          <div className="grid gap-0.5">
            {recentConversations.map((conversation) => {
              const active =
                activeDirectory === defaultWorkspace &&
                conversation.sessionId === activeSessionId;
              return active ? (
                <button
                  aria-current="page"
                  className="flex h-9 w-full items-center gap-2 rounded-lg bg-[#E9E9ED] px-3 text-left text-sm text-accent-foreground"
                  key={conversation.sessionId}
                  onClick={onSelectTask}
                  type="button"
                >
                  <span className="min-w-0 flex-1 truncate">{conversation.title}</span>
                  <span className="text-[10px] text-muted-foreground">{phaseLabels[phase]}</span>
                </button>
              ) : (
                <div
                  className="flex h-9 items-center rounded-lg px-3 text-sm text-muted-foreground"
                  key={conversation.sessionId}
                  title="当前 Pi RPC 暂不支持恢复历史会话"
                >
                  <span className="truncate">{conversation.title}</span>
                </div>
              );
            })}
            {recentConversations.length === 0 && (
              <p className="px-3 py-2 text-xs text-muted-foreground">暂无最近对话</p>
            )}
          </div>
        </section>

        <section aria-labelledby="projects-heading" className="mt-5">
          <div className="flex h-9 items-center px-2">
            <h2 className="text-[13px] font-medium text-muted-foreground" id="projects-heading">
              项目
            </h2>
            <Button
              aria-label="新增项目"
              className="ml-auto size-8"
              disabled={disabled}
              onClick={onNewProject}
              size="icon"
              type="button"
              variant="ghost"
            >
              <PlusIcon className="size-4" />
            </Button>
          </div>

          <div className="grid gap-1">
            {projects.map((project) => {
              const active = project.directory === activeDirectory;
              return (
                <div className="group/project" key={project.directory}>
                  <div className={`flex h-10 items-center rounded-lg px-2 ${active ? "bg-[#E9E9ED]" : "hover:bg-[#EEEEF1]"}`}>
                    <button
                      aria-expanded={active}
                      className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm font-medium outline-none"
                      disabled={disabled}
                      onClick={() => onNewProjectTask(project.directory)}
                      type="button"
                    >
                      <FolderIcon aria-hidden="true" className="size-4 shrink-0" />
                      <span className="truncate">{project.name}</span>
                    </button>
                    <Button
                      aria-label={`在 ${project.name} 中新建任务`}
                      className="size-8 opacity-0 group-hover/project:opacity-100 focus-visible:opacity-100"
                      disabled={disabled}
                      onClick={() => onNewProjectTask(project.directory)}
                      size="icon"
                      type="button"
                      variant="ghost"
                    >
                      <SquarePenIcon className="size-4" />
                    </Button>
                    <Button
                      aria-label={`${project.name} 项目操作`}
                      className="size-8 opacity-0 group-hover/project:opacity-100 focus-visible:opacity-100"
                      disabled
                      size="icon"
                      type="button"
                      variant="ghost"
                    >
                      <MoreHorizontalIcon className="size-4" />
                    </Button>
                  </div>

                  {active && hasTask && (
                    <button
                      aria-current="page"
                      className="mt-0.5 flex h-9 w-full items-center gap-2 rounded-lg bg-[#E9E9ED] py-1 pr-2 pl-9 text-left text-sm text-accent-foreground"
                      onClick={onSelectTask}
                      type="button"
                    >
                      <span className="min-w-0 flex-1 truncate">{taskTitle}</span>
                      <span className="text-[10px] text-muted-foreground">{phaseLabels[phase]}</span>
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </section>
      </div>

      <Button className="mb-3 h-11 w-full justify-start px-5" type="button" variant="ghost">
        <SettingsIcon />
        设置
      </Button>
    </nav>
  );
}
