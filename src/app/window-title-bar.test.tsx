import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const windowMocks = vi.hoisted(() => ({
  close: vi.fn(() => Promise.resolve()),
  minimize: vi.fn(() => Promise.resolve()),
  toggleMaximize: vi.fn(() => Promise.resolve()),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => windowMocks,
}));

import { WindowTitleBar } from "./window-title-bar";

describe("自定义窗口标题栏", () => {
  beforeEach(() => {
    windowMocks.close.mockClear();
    windowMocks.minimize.mockClear();
    windowMocks.toggleMaximize.mockClear();
  });

  it("提供可拖动区域和三个窗口控制按钮", () => {
    render(
      <WindowTitleBar
        collapsed={false}
        hasConversation
        onTitleChange={vi.fn()}
        onToggleSidebar={vi.fn()}
        title="测试任务"
      />,
    );

    expect(screen.getByRole("banner", { name: "应用标题栏" })).toHaveAttribute(
      "data-tauri-drag-region",
    );
    expect(screen.getByRole("banner", { name: "应用标题栏" })).toHaveClass("h-12");

    fireEvent.click(screen.getByRole("button", { name: "最小化窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化或还原窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));

    expect(windowMocks.minimize).toHaveBeenCalledOnce();
    expect(windowMocks.toggleMaximize).toHaveBeenCalledOnce();
    expect(windowMocks.close).toHaveBeenCalledOnce();
  });

  it("只在标题栏提供窗口控制和左侧栏折叠操作", () => {
    const onToggleSidebar = vi.fn();
    const { rerender } = render(
      <WindowTitleBar
        collapsed={false}
        hasConversation
        onTitleChange={vi.fn()}
        onToggleSidebar={onToggleSidebar}
        title="测试任务"
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "折叠左侧栏" }));
    expect(onToggleSidebar).toHaveBeenCalledOnce();
    expect(screen.getByRole("textbox", { name: "任务标题" })).toHaveValue("测试任务");
    expect(screen.getByRole("button", { name: "折叠左侧栏" }).parentElement).toHaveClass("ml-1");

    rerender(
      <WindowTitleBar
        collapsed
        hasConversation={false}
        onTitleChange={vi.fn()}
        onToggleSidebar={onToggleSidebar}
        title="新任务"
      />,
    );
    expect(screen.getByRole("button", { name: "展开左侧栏" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "任务标题" })).not.toBeInTheDocument();
  });
});
