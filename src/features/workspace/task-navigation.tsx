import { useState } from "react";

import { Button } from "@/components/ui/button";
import type {
  ConnectionPhase,
  PiSessionSummary,
  ProjectDescriptor,
} from "@/features/pi-connection";
import { relativeTimeLabel, sessionTitle } from "@/features/workspace/presentation";
import {
  ChevronRightIcon,
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

function directoryName(directory: string | null): string | null {
  if (!directory) return null;
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

export function TaskNavigation({
  open,
  taskTitle,
  phase,
  hasTask,
  onNewTask,
  onNewProject,
  onNewProjectTask,
  onToggleProject,
  onLoadMoreSessions,
  onOpenConversation,
  projects,
  recentSessions,
  projectSessions,
  recentSessionsHasMore,
  projectSessionsHasMore,
  expandedProjects,
  loadingDirectories,
  activeDirectory,
  activeSessionId,
  defaultWorkspace,
  canOpenConversation,
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
  onToggleProject: (directory: string) => void;
  onLoadMoreSessions: (directory: string) => void;
  onOpenConversation: (session: PiSessionSummary) => void;
  projects: ProjectDescriptor[];
  recentSessions: PiSessionSummary[];
  projectSessions: Record<string, PiSessionSummary[]>;
  recentSessionsHasMore: boolean;
  projectSessionsHasMore: Record<string, boolean>;
  expandedProjects: string[];
  loadingDirectories: string[];
  activeDirectory: string | null;
  activeSessionId: string | null;
  defaultWorkspace: string | null;
  canOpenConversation: boolean;
  disabled: boolean;
  onSelectTask: () => void;
  onClose: () => void;
}) {
  const recentFallback = directoryName(defaultWorkspace) ?? "默认工作目录";
  const [recentExpanded, setRecentExpanded] = useState(true);

  return (
    <nav
      aria-label="任务导航"
      className={`fixed top-10 bottom-0 left-0 z-40 flex w-64 shrink-0 flex-col border-r border-[#E8E8EB] bg-[#F6F6F8] transition-transform duration-200 md:static md:translate-x-0 ${
        open ? "translate-x-0" : "-translate-x-full"
      }`}
    >
      <div className="flex h-10 items-center px-3 md:hidden">
        <Button
          aria-expanded={open}
          aria-label="关闭任务导航"
          className="ml-auto"
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
            <button
              aria-expanded={recentExpanded}
              aria-label="最近历史会话"
              className="mr-1 flex size-8 items-center justify-center rounded-lg outline-none hover:bg-[#EEEEF1] focus-visible:ring-[3px] focus-visible:ring-ring/50"
              onClick={() => setRecentExpanded((expanded) => !expanded)}
              type="button"
            >
              <ChevronRightIcon
                aria-hidden="true"
                className={`size-4 text-muted-foreground transition-transform ${recentExpanded ? "rotate-90" : ""}`}
              />
            </button>
            <h2 className="text-[13px] font-medium text-muted-foreground" id="recent-heading">
              最近
            </h2>
            {defaultWorkspace && loadingDirectories.includes(defaultWorkspace) && (
              <span className="ml-auto text-[10px] text-muted-foreground">读取中</span>
            )}
          </div>
          {recentExpanded && (
            <div className="grid gap-0.5">
              {recentSessions.map((session) => {
                const active =
                  activeDirectory === defaultWorkspace && session.id === activeSessionId;
                return (
                  <SessionRow
                    active={active}
                    canOpen={canOpenConversation}
                    fallbackTitle={recentFallback}
                    key={session.path}
                    onOpen={onOpenConversation}
                    onSelectTask={onSelectTask}
                    phase={phase}
                    session={session}
                  />
                );
              })}
              {recentSessionsHasMore && defaultWorkspace && (
                <MoreSessionsButton
                  disabled={loadingDirectories.includes(defaultWorkspace)}
                  onClick={() => onLoadMoreSessions(defaultWorkspace)}
                />
              )}
              {recentSessions.length === 0 && (
                <p className="px-3 py-2 text-xs text-muted-foreground">暂无历史会话</p>
              )}
            </div>
          )}
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
              const expanded = expandedProjects.includes(project.directory);
              const active = project.directory === activeDirectory;
              const sessions = projectSessions[project.directory] ?? [];
              const loading = loadingDirectories.includes(project.directory);
              const currentSessionListed = sessions.some(
                (session) => session.id === activeSessionId,
              );
              return (
                <div className="group/project" key={project.directory}>
                  <div className={`flex h-10 items-center rounded-lg px-2 ${expanded ? "bg-[#EEEEF1]" : "hover:bg-[#EEEEF1]"}`}>
                    <button
                      aria-expanded={expanded}
                      aria-label={`${project.name} 历史会话`}
                      className="flex min-w-0 flex-1 items-center gap-2 text-left text-sm font-medium outline-none"
                      disabled={disabled}
                      onClick={() => onToggleProject(project.directory)}
                      type="button"
                    >
                      <ChevronRightIcon
                        aria-hidden="true"
                        className={`size-4 shrink-0 text-muted-foreground transition-transform ${expanded ? "rotate-90" : ""}`}
                      />
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

                  {expanded && (
                    <div
                      aria-label={`${project.name}历史会话列表`}
                      className="mt-0.5 grid gap-0.5 pl-6"
                      role="region"
                    >
                      {active && hasTask && !currentSessionListed && (
                        <button
                          aria-current="page"
                          className="flex h-9 w-full items-center gap-2 rounded-lg bg-[#E9E9ED] px-3 text-left text-sm text-accent-foreground"
                          onClick={onSelectTask}
                          type="button"
                        >
                          <span className="min-w-0 flex-1 truncate">{taskTitle}</span>
                          <span className="text-[10px] text-muted-foreground">{phaseLabels[phase]}</span>
                        </button>
                      )}
                      {sessions.map((session) => (
                        <SessionRow
                          active={active && session.id === activeSessionId}
                          canOpen={canOpenConversation}
                          fallbackTitle={project.name}
                          key={session.path}
                          onOpen={onOpenConversation}
                          onSelectTask={onSelectTask}
                          phase={phase}
                          session={session}
                        />
                      ))}
                      {projectSessionsHasMore[project.directory] && (
                        <MoreSessionsButton
                          disabled={loading}
                          onClick={() => onLoadMoreSessions(project.directory)}
                        />
                      )}
                      {loading && sessions.length === 0 && (
                        <p className="px-3 py-2 text-xs text-muted-foreground">正在读取历史会话…</p>
                      )}
                      {!loading && sessions.length === 0 && (
                        <p className="px-3 py-2 text-xs text-muted-foreground">暂无历史会话</p>
                      )}
                    </div>
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

function MoreSessionsButton({
  disabled,
  onClick,
}: {
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      aria-label="显示更多"
      className="flex h-9 w-full items-center justify-center rounded-lg px-3 text-xs font-medium text-[#6C6C72] outline-none hover:bg-[#EEEEF1] hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:text-[#6C6C72]"
      disabled={disabled}
      onClick={onClick}
      type="button"
    >
      <span>显示更多</span>
    </button>
  );
}

function SessionRow({
  session,
  active,
  canOpen,
  fallbackTitle,
  phase,
  onOpen,
  onSelectTask,
}: {
  session: PiSessionSummary;
  active: boolean;
  canOpen: boolean;
  fallbackTitle: string;
  phase: ConnectionPhase;
  onOpen: (session: PiSessionSummary) => void;
  onSelectTask: () => void;
}) {
  const title = sessionTitle(session, fallbackTitle);

  if (active) {
    return (
      <button
        aria-current="page"
        className="flex h-9 w-full items-center gap-2 rounded-lg bg-[#E9E9ED] px-3 text-left text-sm text-accent-foreground"
        onClick={onSelectTask}
        type="button"
      >
        <span className="min-w-0 flex-1 truncate">{title}</span>
        <span className="text-[10px] text-muted-foreground">{phaseLabels[phase]}</span>
      </button>
    );
  }

  return (
    <button
      className="flex h-9 w-full items-center gap-2 rounded-lg px-3 text-left text-sm text-muted-foreground enabled:hover:bg-[#EEEEF1] disabled:opacity-60"
      disabled={!canOpen}
      onClick={() => onOpen(session)}
      title={session.firstMessage ?? undefined}
      type="button"
    >
      <span className="min-w-0 flex-1 truncate">{title}</span>
      <span className="shrink-0 text-[10px] text-muted-foreground">
        {relativeTimeLabel(session.modifiedAtMs)}
      </span>
    </button>
  );
}
