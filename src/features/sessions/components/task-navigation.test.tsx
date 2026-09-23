import { fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { PiSessionSummary } from "@/lib/pi-rpc/types";
import type { ConnectionPhase } from "@/lib/pi-rpc/types";
import type { SessionTransition } from "@/features/sessions/sessions-slice";
import { TaskNavigation } from "./task-navigation";

// jsdom 不做布局，clientWidth 恒为 0；用可控的 scrollWidth 模拟标题内容宽度。
function stubElementOverflow(width: number) {
  Object.defineProperty(HTMLElement.prototype, "scrollWidth", {
    configurable: true,
    get: () => width,
  });
}

afterEach(() => {
  delete (HTMLElement.prototype as { scrollWidth?: number }).scrollWidth;
});

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
  sessionTransition = null,
  phase = "ready",
  activeSessionId = null,
}: {
  recentSessions?: PiSessionSummary[];
  recentSessionsHasMore?: boolean;
  projectSessions?: Record<string, PiSessionSummary[]>;
  projectSessionsHasMore?: Record<string, boolean>;
  expandedProjects?: string[];
  sessionTransition?: SessionTransition;
  phase?: ConnectionPhase;
  activeSessionId?: string | null;
} = {}) {
  const onLoadMoreSessions = vi.fn();
  const onNewTask = vi.fn();
  render(
    <TaskNavigation
      activeDirectory="/tmp/lure"
      activeSessionId={activeSessionId}
      canOpenConversation
      defaultWorkspace="/tmp/lure"
      disabled={false}
      expandedProjects={expandedProjects}
      hasTask={false}
      loadingDirectories={[]}
      onNewProject={vi.fn()}
      onNewProjectTask={vi.fn()}
      onNewTask={onNewTask}
      onLoadMoreSessions={onLoadMoreSessions}
      onOpenConversation={vi.fn()}
      onOpenSettings={vi.fn()}
      onToggleProject={vi.fn()}
      phase={phase}
      projects={projectSessions["/tmp/project"] ? [{ name: "示例项目", directory: "/tmp/project" }] : []}
      projectSessions={projectSessions}
      projectSessionsHasMore={projectSessionsHasMore}
      recentSessions={recentSessions}
      recentSessionsHasMore={recentSessionsHasMore}
      sessionTransition={sessionTransition}
      taskTitle="当前任务"
    />,
  );
  return { onLoadMoreSessions, onNewTask };
}

describe("任务导航会话列表", () => {
  it("最近显示后端返回的一页，并从显示更多请求下一页", () => {
    const { onLoadMoreSessions } = renderNavigation();
    const recent = screen.getByRole("region", { name: "最近" });

    expect(within(recent).getAllByRole("button", { name: /会话 \d/ })).toHaveLength(3);
    expect(within(recent).getByText("会话 1")).toBeInTheDocument();
    expect(within(recent).queryByText("会话 4")).not.toBeInTheDocument();

    fireEvent.click(within(recent).getByRole("button", { name: "显示更多最近会话" }));
    expect(onLoadMoreSessions).toHaveBeenCalledWith("/tmp/lure");
  });

  it("最近标题右侧提供更多和新建任务操作", () => {
    const { onNewTask } = renderNavigation();
    const recent = screen.getByRole("region", { name: "最近" });

    expect(within(recent).getByRole("button", { name: "显示更多最近会话" })).toBeInTheDocument();
    fireEvent.click(within(recent).getByRole("button", { name: "在默认工作目录中新建任务" }));
    expect(onNewTask).toHaveBeenCalledOnce();
  });

  it("最近会话使用醒目的单列标题，不显示时间", () => {
    renderNavigation();
    const recent = screen.getByRole("region", { name: "最近" });
    const row = within(recent).getByRole("button", { name: "会话 1" });

    expect(row).toHaveClass("h-10", "text-[15px]");
    expect(row).not.toHaveTextContent("刚刚");
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

  it("展开的项目会话显示更多并请求项目下一页", () => {
    const { onLoadMoreSessions } = renderNavigation({
      recentSessions: [],
      recentSessionsHasMore: false,
      projectSessions: { "/tmp/project": sessions("/tmp/project", 3) },
      projectSessionsHasMore: { "/tmp/project": true },
      expandedProjects: ["/tmp/project"],
    });
    const project = screen.getByRole("region", { name: "示例项目历史会话列表" });

    expect(within(project).getAllByRole("button", { name: /会话 \d/ })).toHaveLength(3);
    const showMore = within(project).getByRole("button", { name: "显示更多" });
    expect(showMore).toHaveTextContent("显示更多");
    fireEvent.click(showMore);
    expect(onLoadMoreSessions).toHaveBeenCalledWith("/tmp/project");
  });

  it("切换中的会话保持静默，仅禁止重复点击其他会话", () => {
    renderNavigation({
      sessionTransition: "/tmp/lure/sessions/2.jsonl",
    });

    const recent = screen.getByRole("region", { name: "最近" });
    const opening = within(recent).getByRole("button", { name: /会话 2/ });
    expect(opening).toHaveAttribute("aria-busy", "true");
    // 不再显示“正在打开”这类连接过程提示，条目也不显示时间列。
    expect(within(recent).queryByText(/正在打开|正在连接/)).not.toBeInTheDocument();
    expect(opening).not.toHaveTextContent("刚刚");
    expect(within(recent).getByRole("button", { name: /会话 1/ })).toBeDisabled();
    // 切换期间列表本身不消失，仍显示全部已加载记录。
    expect(within(recent).getAllByRole("button", { name: /会话 \d/ })).toHaveLength(3);
  });

  it("导航不展示 RPC 连接状态标签", () => {
    renderNavigation();

    expect(screen.queryByText(/已连接|正在连接|未连接|连接失败/)).not.toBeInTheDocument();
  });

  it("最近列表不在会话行内展示运行状态", () => {
    renderNavigation({ activeSessionId: "session-1", phase: "running" });

    const recent = screen.getByRole("region", { name: "最近" });
    expect(within(recent).queryByText("执行中")).not.toBeInTheDocument();
  });

  it("悬停溢出标题的会话行时启动跑马灯滚动全文", () => {
    stubElementOverflow(160);
    renderNavigation();

    const recent = screen.getByRole("region", { name: "最近" });
    const row = within(recent).getByRole("button", { name: /会话 1/ });
    fireEvent.mouseEnter(row);

    const title = within(row).getByText("会话 1");
    expect(title).toHaveClass("animate-marquee");

    fireEvent.mouseLeave(row);
    expect(title).not.toHaveClass("animate-marquee");
  });

  it("会话行不再用原生悬浮提示，超长标题由跑马灯承担", () => {
    renderNavigation();

    const recent = screen.getByRole("region", { name: "最近" });
    for (const row of within(recent).getAllByRole("button", { name: /会话 \d/ })) {
      expect(row).not.toHaveAttribute("title");
    }
  });
});
