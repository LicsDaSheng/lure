import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { PiSessionSummary } from "@/features/pi-connection";
import { TaskNavigation } from "@/features/workspace/task-navigation";

function sessions(directory: string, count: number): PiSessionSummary[] {
  return Array.from({ length: count }, (_, index) => ({
    path: `${directory}/sessions/${index + 1}.jsonl`,
    id: `session-${index + 1}`,
    cwd: directory,
    name: null,
    parentSessionPath: null,
    createdAtMs: 2_000_000_000_000 - index,
    modifiedAtMs: 2_000_000_000_000 - index,
    messageCount: 1,
    firstMessage: `会话 ${index + 1}`,
  }));
}

function renderNavigation({
  recentSessions = sessions("/tmp/lure", 3),
  recentSessionsHasMore = true,
  projectSessions = {},
  projectSessionsHasMore = {},
  expandedProjects = [],
}: {
  recentSessions?: PiSessionSummary[];
  recentSessionsHasMore?: boolean;
  projectSessions?: Record<string, PiSessionSummary[]>;
  projectSessionsHasMore?: Record<string, boolean>;
  expandedProjects?: string[];
} = {}) {
  const onLoadMoreSessions = vi.fn();
  render(
    <TaskNavigation
      activeDirectory="/tmp/lure"
      activeSessionId={null}
      canOpenConversation
      defaultWorkspace="/tmp/lure"
      disabled={false}
      expandedProjects={expandedProjects}
      hasTask={false}
      loadingDirectories={[]}
      onClose={vi.fn()}
      onNewProject={vi.fn()}
      onNewProjectTask={vi.fn()}
      onNewTask={vi.fn()}
      onLoadMoreSessions={onLoadMoreSessions}
      onOpenConversation={vi.fn()}
      onSelectTask={vi.fn()}
      onToggleProject={vi.fn()}
      open
      phase="ready"
      projects={projectSessions["/tmp/project"] ? [{ name: "示例项目", directory: "/tmp/project" }] : []}
      projectSessions={projectSessions}
      projectSessionsHasMore={projectSessionsHasMore}
      recentSessions={recentSessions}
      recentSessionsHasMore={recentSessionsHasMore}
      taskTitle="当前任务"
    />,
  );
  return { onLoadMoreSessions };
}

describe("任务导航会话列表", () => {
  it("最近显示后端返回的一页，并从更多消息请求下一页", () => {
    const { onLoadMoreSessions } = renderNavigation();
    const recent = screen.getByRole("region", { name: "最近" });

    expect(within(recent).getAllByRole("button", { name: /会话 \d/ })).toHaveLength(3);
    expect(within(recent).getByText("会话 1")).toBeInTheDocument();
    expect(within(recent).queryByText("会话 4")).not.toBeInTheDocument();

    fireEvent.click(within(recent).getByRole("button", { name: "更多消息" }));
    expect(onLoadMoreSessions).toHaveBeenCalledWith("/tmp/lure");
  });

  it("最近分组可以折叠和重新展开", () => {
    renderNavigation();
    const toggle = screen.getByRole("button", { name: "最近历史会话" });

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("会话 1")).not.toBeInTheDocument();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("会话 1")).toBeInTheDocument();
  });

  it("展开的项目会话显示更多消息并请求项目下一页", () => {
    const { onLoadMoreSessions } = renderNavigation({
      recentSessions: [],
      recentSessionsHasMore: false,
      projectSessions: { "/tmp/project": sessions("/tmp/project", 3) },
      projectSessionsHasMore: { "/tmp/project": true },
      expandedProjects: ["/tmp/project"],
    });
    const project = screen.getByRole("region", { name: "示例项目历史会话列表" });

    expect(within(project).getAllByRole("button", { name: /会话 \d/ })).toHaveLength(3);
    fireEvent.click(within(project).getByRole("button", { name: "更多消息" }));
    expect(onLoadMoreSessions).toHaveBeenCalledWith("/tmp/project");
  });
});
