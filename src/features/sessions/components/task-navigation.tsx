import { useState } from "react";

import { Button } from "@/components/ui/button";
import { MarqueeText } from "@/components/marquee-text";
import type {
  ConnectionPhase,
  PiSessionSummary,
  ProjectDescriptor,
} from "@/lib/pi-rpc/types";
import {
  directoryName,
  relativeTimeLabel,
  sessionTitle,
} from "@/features/sessions/session-presentation";
import type { SessionTransition } from "@/features/sessions/sessions-slice";
import {
  ChevronRightIcon,
  FolderIcon,
  FolderOpenIcon,
  MessageSquareIcon,
  MoreHorizontalIcon,
  PlusIcon,
  SettingsIcon,
  SquarePenIcon,
} from "lucide-react";

export function TaskNavigation({
  taskTitle,
  phase,
  hasTask,
  onNewTask,
  onNewProject,
  onNewProjectTask,
  onToggleProject,
  onLoadMoreSessions,
  onOpenConversation,
  onOpenSettings,
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
  sessionTransition,
  activeView,
  onActiveViewChange,
}: {
  taskTitle: string;
  phase: ConnectionPhase;
  hasTask: boolean;
  onNewTask: () => void;
  onNewProject: () => void;
  onNewProjectTask: (directory: string) => void;
  onToggleProject: (directory: string) => void;
  onLoadMoreSessions: (directory: string) => void;
  onOpenConversation: (session: PiSessionSummary) => void;
  onOpenSettings: () => void;
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
  sessionTransition: SessionTransition;
  activeView: "conversations" | "projects";
  onActiveViewChange: (view: "conversations" | "projects") => void;
}) {
  const recentFallback = directoryName(defaultWorkspace) ?? "默认工作目录";
  const [recentExpanded, setRecentExpanded] = useState(true);
  // 整个切换事务期间不接受新的切换请求，避免并发重建 RPC。
  const canSwitchSession = canOpenConversation && !sessionTransition;

  return (
    <nav
      aria-label="任务导航"
      className="flex w-[272px] shrink-0 flex-col border-r border-border bg-sidebar"
    >
      <div
        aria-label="导航视图"
        className="mx-3 mt-3 grid h-10 grid-cols-2 rounded-xl bg-muted p-1"
        role="tablist"
      >
        <button
          aria-controls="conversations-panel"
          aria-selected={activeView === "conversations"}
          className="flex min-w-0 items-center justify-center gap-2 rounded-lg text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-selected:bg-background aria-selected:font-medium aria-selected:text-foreground aria-selected:shadow-sm"
          disabled={disabled && activeView !== "conversations"}
          id="conversations-tab"
          onClick={() => onActiveViewChange("conversations")}
          role="tab"
          tabIndex={activeView === "conversations" ? 0 : -1}
          type="button"
        >
          <MessageSquareIcon className="size-[18px]" strokeWidth={1.75} />
          对话
        </button>
        <button
          aria-controls="projects-panel"
          aria-selected={activeView === "projects"}
          className="flex min-w-0 items-center justify-center gap-2 rounded-lg text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-[3px] focus-visible:ring-ring/50 aria-selected:bg-background aria-selected:font-medium aria-selected:text-foreground aria-selected:shadow-sm"
          disabled={disabled && activeView !== "projects"}
          id="projects-tab"
          onClick={() => onActiveViewChange("projects")}
          role="tab"
          tabIndex={activeView === "projects" ? 0 : -1}
          type="button"
        >
          <FolderIcon className="size-[18px]" strokeWidth={1.75} />
          项目
        </button>
      </div>

      {activeView === "conversations" ? (
        <div
          aria-labelledby="conversations-tab"
          className="min-h-0 flex-1 overflow-y-auto"
          id="conversations-panel"
          role="tabpanel"
        >
          <div className="mt-3 border-b border-border px-3 pb-3">
            <Button
              className="h-11 w-full justify-start px-3"
              onClick={onNewTask}
              disabled={disabled}
              type="button"
              variant="ghost"
            >
              <PlusIcon />
              新建对话
            </Button>
          </div>

          <section aria-labelledby="recent-heading" className="mx-3 mt-5">
            <div className="flex h-10 items-center px-1">
              <button
                aria-expanded={recentExpanded}
                aria-label="最近历史会话"
                className="flex h-9 min-w-0 items-center gap-1 rounded-lg px-2 text-xs font-medium text-muted-foreground outline-none hover:bg-[var(--button-subtle-hover)] active:bg-[var(--button-subtle-active)] focus-visible:ring-[3px] focus-visible:ring-ring/50"
                onClick={() => setRecentExpanded((expanded) => !expanded)}
                type="button"
              >
                <h2 id="recent-heading">最近</h2>
                <ChevronRightIcon
                  aria-hidden="true"
                  className={`size-4 shrink-0 transition-transform ${recentExpanded ? "rotate-90" : ""}`}
                />
              </button>
              {defaultWorkspace &&
                loadingDirectories.includes(defaultWorkspace) && (
                  <span className="ml-1 text-[10px] text-muted-foreground">
                    读取中
                  </span>
                )}
              <div className="ml-auto flex items-center">
                <Button
                  aria-label="显示更多最近会话"
                  className="size-9 text-muted-foreground"
                  disabled={
                    !recentSessionsHasMore ||
                    !defaultWorkspace ||
                    loadingDirectories.includes(defaultWorkspace)
                  }
                  onClick={() =>
                    defaultWorkspace && onLoadMoreSessions(defaultWorkspace)
                  }
                  size="icon"
                  type="button"
                  variant="ghost"
                >
                  <MoreHorizontalIcon className="size-[18px]" />
                </Button>
                <Button
                  aria-label="在默认工作目录中新建对话"
                  className="size-9 text-muted-foreground"
                  disabled={disabled}
                  onClick={onNewTask}
                  size="icon"
                  type="button"
                  variant="ghost"
                >
                  <SquarePenIcon className="size-[18px]" />
                </Button>
              </div>
            </div>
            {recentExpanded && (
              <div className="mt-1 grid gap-0.5">
                {recentSessions.map((session) => {
                  const active =
                    activeDirectory === defaultWorkspace &&
                    session.id === activeSessionId;
                  return (
                    <SessionRow
                      active={active}
                      canOpen={canSwitchSession}
                      fallbackTitle={recentFallback}
                      key={session.path}
                      onOpen={onOpenConversation}
                      phase={phase}
                      recent
                      session={session}
                      transitioning={sessionTransition === session.path}
                    />
                  );
                })}
                {recentSessions.length === 0 && (
                  <p className="px-3 py-2 text-xs text-muted-foreground">
                    暂无历史会话
                  </p>
                )}
              </div>
            )}
          </section>
        </div>
      ) : (
        <div
          aria-labelledby="projects-tab"
          className="min-h-0 flex-1 overflow-y-auto px-3"
          id="projects-panel"
          role="tabpanel"
        >
          <section aria-labelledby="projects-heading" className="mt-4">
            <div className="group/projects-heading flex h-9 items-center px-2">
              <h2
                className="text-xs font-medium text-muted-foreground"
                id="projects-heading"
              >
                项目
              </h2>
              <Button
                aria-label="新增项目"
                className="ml-auto size-8 opacity-0 transition-opacity group-hover/projects-heading:opacity-100 focus-visible:opacity-100"
                disabled={disabled}
                onClick={onNewProject}
                size="icon"
                type="button"
                variant="ghost"
              >
                <PlusIcon className="size-4" />
              </Button>
            </div>

            <div className="-mx-1 grid gap-0.5">
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
                    <div className="flex h-10 items-center rounded-lg px-2 hover:bg-[var(--button-subtle-hover)] focus-within:bg-[var(--button-subtle-hover)]">
                      <button
                        aria-expanded={expanded}
                        aria-label={`${project.name} 历史会话`}
                        className="flex h-full min-w-0 flex-1 items-center gap-2.5 rounded-lg text-left text-xs font-normal outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50"
                        disabled={disabled}
                        onClick={() => onToggleProject(project.directory)}
                        type="button"
                      >
                        {expanded ? (
                          <FolderOpenIcon
                            aria-hidden="true"
                            className="size-5 shrink-0"
                            strokeWidth={1.75}
                          />
                        ) : (
                          <FolderIcon
                            aria-hidden="true"
                            className="size-5 shrink-0"
                            strokeWidth={1.75}
                          />
                        )}
                        <span className="truncate">{project.name}</span>
                      </button>
                      <Button
                        aria-label={`在 ${project.name} 中新建任务`}
                        className="size-8 shrink-0 opacity-0 transition-opacity group-hover/project:opacity-100 focus-visible:opacity-100"
                        disabled={disabled}
                        onClick={() => onNewProjectTask(project.directory)}
                        size="icon"
                        type="button"
                        variant="ghost"
                      >
                        <SquarePenIcon className="size-4" />
                      </Button>
                    </div>

                    {expanded && (
                      <div
                        aria-label={`${project.name}历史会话列表`}
                        className="mt-0.5 grid gap-0.5"
                        role="region"
                      >
                        {/* 切换事务进行中不弹“当前任务”兔底行：连接此刻指向的临时会话
                          不是用户要打开的目标，避免列表闪现一条以项目名命名的条目。 */}
                        {active &&
                          hasTask &&
                          !currentSessionListed &&
                          !sessionTransition && (
                            <CurrentTaskFallbackRow
                              phase={phase}
                              taskTitle={taskTitle}
                            />
                          )}
                        {sessions.map((session) => (
                          <SessionRow
                            active={active && session.id === activeSessionId}
                            canOpen={canSwitchSession}
                            fallbackTitle={project.name}
                            key={session.path}
                            onOpen={onOpenConversation}
                            phase={phase}
                            nested
                            session={session}
                            transitioning={sessionTransition === session.path}
                          />
                        ))}
                        {projectSessionsHasMore[project.directory] && (
                          <MoreSessionsButton
                            disabled={loading}
                            nested
                            onClick={() =>
                              onLoadMoreSessions(project.directory)
                            }
                          />
                        )}
                        {loading && sessions.length === 0 && (
                          <p className="py-2 pr-3 pl-[38px] text-xs text-muted-foreground">
                            正在读取历史会话…
                          </p>
                        )}
                        {!loading && sessions.length === 0 && (
                          <p className="py-2 pr-3 pl-[38px] text-xs text-muted-foreground">
                            暂无历史会话
                          </p>
                        )}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </section>
        </div>
      )}

      <Button
        className="mb-3 h-11 w-full justify-start px-5"
        onClick={onOpenSettings}
        type="button"
        variant="ghost"
      >
        <SettingsIcon />
        设置
      </Button>
    </nav>
  );
}

function CurrentTaskFallbackRow({
  phase,
  taskTitle,
}: {
  phase: ConnectionPhase;
  taskTitle: string;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      aria-current="page"
      className="marquee-row flex h-9 w-full items-center gap-2 rounded-lg bg-[var(--button-subtle-active)] pr-3 pl-[38px] text-left text-[13px] font-medium text-accent-foreground"
      onBlur={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      type="button"
    >
      <MarqueeText hovered={hovered} text={taskTitle} />
      {phase === "running" && (
        <span className="w-16 shrink-0 text-right text-[10px] text-muted-foreground">
          执行中
        </span>
      )}
    </button>
  );
}

function MoreSessionsButton({
  disabled,
  nested = false,
  onClick,
}: {
  disabled: boolean;
  nested?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      aria-label="显示更多"
      className={`flex h-9 w-full items-center justify-start rounded-lg pr-3 text-[13px] font-normal text-[var(--button-disabled-foreground)] outline-none hover:bg-[var(--button-subtle-hover)] hover:text-muted-foreground active:bg-[var(--button-subtle-active)] focus-visible:ring-[3px] focus-visible:ring-ring/50 disabled:text-[var(--button-disabled-foreground)] ${nested ? "pl-[38px]" : "pl-3"}`}
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
  nested = false,
  phase,
  recent = false,
  onOpen,
  transitioning = false,
}: {
  session: PiSessionSummary;
  active: boolean;
  canOpen: boolean;
  fallbackTitle: string;
  nested?: boolean;
  phase: ConnectionPhase;
  recent?: boolean;
  onOpen: (session: PiSessionSummary) => void;
  transitioning?: boolean;
}) {
  const [hovered, setHovered] = useState(false);
  const title = sessionTitle(session, fallbackTitle);
  // 连接过程静默：导航不展示连接状态，只在当前会话执行中时提示。
  const activeStatus =
    phase === "running" ? "执行中" : relativeTimeLabel(session.modifiedAtMs);
  // 时间列固定宽度右对齐，标题列占行内固定剩余宽度，不随时间文案伸缩。
  const statusClassName =
    "w-16 shrink-0 text-right text-[10px] text-muted-foreground";

  if (active) {
    return (
      <button
        aria-busy={transitioning}
        aria-current="page"
        className={`marquee-row flex w-full items-center rounded-lg bg-[var(--button-subtle-active)] text-left font-medium text-accent-foreground ${recent ? "h-10 px-3 text-[15px] leading-6" : `h-9 gap-2 pr-3 text-[13px] ${nested ? "pl-[38px]" : "pl-3"}`}`}
        onBlur={() => setHovered(false)}
        onFocus={() => setHovered(true)}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
        type="button"
      >
        <MarqueeText hovered={hovered} text={title} />
        {!recent && <span className={statusClassName}>{activeStatus}</span>}
      </button>
    );
  }

  return (
    <button
      aria-busy={transitioning}
      className={`marquee-row flex w-full items-center rounded-lg text-left text-foreground enabled:hover:bg-[var(--button-subtle-hover)] enabled:active:bg-[var(--button-subtle-active)] disabled:text-[var(--button-disabled-foreground)] ${recent ? "h-10 px-3 text-[15px] leading-6" : `h-9 gap-2 pr-3 text-[13px] ${nested ? "pl-[38px]" : "pl-3"}`}`}
      disabled={!canOpen}
      onClick={() => onOpen(session)}
      onBlur={() => setHovered(false)}
      onFocus={() => setHovered(true)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      type="button"
    >
      <MarqueeText hovered={hovered} text={title} />
      {!recent && (
        <span className={statusClassName}>
          {relativeTimeLabel(session.modifiedAtMs)}
        </span>
      )}
    </button>
  );
}
