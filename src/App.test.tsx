import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import type { EventCallback } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => {
  const listeners = new Map<string, EventCallback<unknown>>();
  const disposables: Array<ReturnType<typeof vi.fn>> = [];
  const terminal = {
    dispose: vi.fn(),
    focus: vi.fn(),
    loadAddon: vi.fn(),
    onData: vi.fn(),
    open: vi.fn(),
    write: vi.fn(),
  };
  const fitAddon = {
    dispose: vi.fn(),
    fit: vi.fn(),
    proposeDimensions: vi.fn(() => ({ cols: 100, rows: 30 })),
  };
  return {
    disposables,
    fitAddon,
    invoke: vi.fn(),
    listen: vi.fn(async (name: string, callback: EventCallback<unknown>) => {
      listeners.set(name, callback);
      const unlisten = vi.fn();
      disposables.push(unlisten);
      return unlisten;
    }),
    listeners,
    open: vi.fn(),
    terminal,
    terminalOptions: null as Record<string, unknown> | null,
    window: {
      close: vi.fn(async () => undefined),
      minimize: vi.fn(async () => undefined),
      toggleMaximize: vi.fn(async () => undefined),
    },
  };
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => mocks.window,
}));
vi.mock("@xterm/xterm", () => ({
  Terminal: class TerminalMock {
    constructor(options: Record<string, unknown>) {
      mocks.terminalOptions = options;
      return mocks.terminal;
    }
  },
}));
vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class FitAddonMock {
    constructor() {
      return mocks.fitAddon;
    }
  },
}));

import App from "./App";

function emit(name: string, payload: unknown) {
  const callback = mocks.listeners.get(name);
  if (!callback) throw new Error(`未订阅事件：${name}`);
  act(() => callback({ event: name, id: 1, payload }));
}

describe("Pi 桌面终端", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listeners.clear();
    mocks.disposables.length = 0;
    mocks.terminalOptions = null;
    mocks.fitAddon.proposeDimensions.mockReturnValue({ cols: 100, rows: 30 });
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "get_default_workspace") return "/workspace";
      if (command === "start_terminal") {
        return { sessionId: 7, workingDirectory: "/workspace" };
      }
      return undefined;
    });
  });

  it("使用浅色 Gruvbox 配置桌面和终端主题", () => {
    const { container } = render(<App />);

    expect(container.firstElementChild).toHaveClass(
      "bg-[#fbf1c7]",
      "text-[#3c3836]",
    );
    expect(mocks.terminalOptions?.theme).toEqual({
      background: "#fbf1c7",
      foreground: "#3c3836",
      cursor: "#3c3836",
      cursorAccent: "#fbf1c7",
      selectionBackground: "#45858855",
      black: "#fbf1c7",
      red: "#cc241d",
      green: "#98971a",
      yellow: "#d79921",
      blue: "#458588",
      magenta: "#b16286",
      cyan: "#689d6a",
      white: "#7c6f64",
      brightBlack: "#928374",
      brightRed: "#9d0006",
      brightGreen: "#79740e",
      brightYellow: "#b57614",
      brightBlue: "#076678",
      brightMagenta: "#8f3f71",
      brightCyan: "#427b58",
      brightWhite: "#3c3836",
    });
  });

  it("挂载 xterm 并在默认工作目录启动 Pi PTY", async () => {
    render(<App />);

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("start_terminal", {
        columns: 100,
        rows: 30,
        workingDirectory: "/workspace",
      }),
    );

    expect(mocks.terminal.loadAddon).toHaveBeenCalledWith(mocks.fitAddon);
    expect(mocks.terminal.open).toHaveBeenCalledTimes(1);
    expect(mocks.fitAddon.fit).toHaveBeenCalled();
    expect(mocks.terminal.focus).toHaveBeenCalled();
    expect(screen.getByText("/workspace")).toBeInTheDocument();
  });

  it("双向转发终端字节、输入与 resize", async () => {
    render(<App />);
    await waitFor(() => expect(mocks.terminal.onData).toHaveBeenCalled());

    const onData = mocks.terminal.onData.mock.calls[0][0] as (
      data: string,
    ) => void;
    onData("/help\r");
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("write_terminal", {
        data: "/help\r",
        sessionId: 7,
      }),
    );

    emit("lure://terminal-output", {
      sessionId: 7,
      data: [27, 91, 51, 49, 109, 80, 105],
    });
    expect(mocks.terminal.write).toHaveBeenCalledWith(
      new Uint8Array([27, 91, 51, 49, 109, 80, 105]),
    );

    fireEvent(window, new Event("resize"));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("resize_terminal", {
        columns: 100,
        rows: 30,
        sessionId: 7,
      }),
    );
  });

  it("选择新目录时停止旧终端并在新目录重启", async () => {
    mocks.open.mockResolvedValue("/new-project");
    mocks.invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "get_default_workspace") return "/workspace";
      if (command === "start_terminal") {
        const directory = (args as { workingDirectory: string })
          .workingDirectory;
        return {
          sessionId: directory === "/workspace" ? 7 : 8,
          workingDirectory: directory,
        };
      }
      return undefined;
    });

    render(<App />);
    await screen.findByText("/workspace");
    fireEvent.click(screen.getByRole("button", { name: "选择工作目录" }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("stop_terminal", {
        sessionId: 7,
      }),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("start_terminal", {
        columns: 100,
        rows: 30,
        workingDirectory: "/new-project",
      }),
    );
    expect(await screen.findByText("/new-project")).toBeInTheDocument();
  });

  it("Pi 退出后保留终端并允许重新启动", async () => {
    render(<App />);
    await screen.findByText("/workspace");

    emit("lure://terminal-exit", {
      sessionId: 7,
      exitCode: 0,
      success: true,
    });

    expect(await screen.findByText("Pi 已退出（代码 0）")).toBeInTheDocument();
    const restartButtons = screen.getAllByRole("button", {
      name: "重新启动 Pi",
    });
    fireEvent.click(restartButtons.at(-1)!);
    await waitFor(() => {
      const starts = mocks.invoke.mock.calls.filter(
        ([command]) => command === "start_terminal",
      );
      expect(starts).toHaveLength(2);
    });
  });

  it("启动失败时给出原因和恢复操作", async () => {
    mocks.invoke.mockImplementation(async (command: string) => {
      if (command === "get_default_workspace") return "/workspace";
      if (command === "start_terminal") throw new Error("找不到 Pi 可执行文件");
    });

    render(<App />);

    expect(await screen.findByText("无法启动 Pi")).toBeInTheDocument();
    expect(screen.getByText(/找不到 Pi 可执行文件/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "重试启动" }),
    ).toBeInTheDocument();
  });

  it("停止按钮终止当前 Pi，且忽略旧会话输出", async () => {
    render(<App />);
    await screen.findByText("/workspace");

    emit("lure://terminal-output", {
      sessionId: 99,
      data: [65, 66, 67],
    });
    expect(mocks.terminal.write).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "停止 Pi" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("stop_terminal", {
        sessionId: 7,
      }),
    );
  });

  it("取消目录选择不重启，窗口控制调用原生窗口 API", async () => {
    mocks.open.mockResolvedValue(null);
    render(<App />);
    await screen.findByText("/workspace");

    fireEvent.click(screen.getByRole("button", { name: "选择工作目录" }));
    await waitFor(() => expect(mocks.open).toHaveBeenCalled());
    expect(
      mocks.invoke.mock.calls.filter(
        ([command]) => command === "start_terminal",
      ),
    ).toHaveLength(1);

    fireEvent.click(screen.getByRole("button", { name: "关闭窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最小化窗口" }));
    fireEvent.click(screen.getByRole("button", { name: "最大化或还原窗口" }));
    expect(mocks.window.close).toHaveBeenCalled();
    expect(mocks.window.minimize).toHaveBeenCalled();
    expect(mocks.window.toggleMaximize).toHaveBeenCalled();
  });

  it("卸载时解除事件订阅并释放 xterm", async () => {
    const { unmount } = render(<App />);
    await waitFor(() => expect(mocks.disposables).toHaveLength(2));

    unmount();

    await waitFor(() =>
      expect(
        mocks.disposables.every((dispose) => dispose.mock.calls.length === 1),
      ).toBe(true),
    );
    expect(mocks.terminal.dispose).toHaveBeenCalled();
    expect(mocks.fitAddon.dispose).toHaveBeenCalled();
  });
});
