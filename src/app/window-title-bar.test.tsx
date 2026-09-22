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
    render(<WindowTitleBar />);

    expect(screen.getByRole("banner", { name: "应用标题栏" })).toHaveAttribute(
      "data-tauri-drag-region",
    );

    fireEvent.click(screen.getByRole("button", { name: "最小化窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化或还原窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));

    expect(windowMocks.minimize).toHaveBeenCalledOnce();
    expect(windowMocks.toggleMaximize).toHaveBeenCalledOnce();
    expect(windowMocks.close).toHaveBeenCalledOnce();
  });
});
