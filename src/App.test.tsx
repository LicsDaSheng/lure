import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { EventEnvelope } from "@/features/pi-connection/reducer";
import App from "./App";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  eventHandler: undefined as
    | ((event: { payload: EventEnvelope }) => void)
    | undefined,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

const readySnapshot = {
  phase: "ready",
  workingDirectory: "/tmp/lure-project",
  sessionId: "session-1",
  sessionFile: "/tmp/session.jsonl",
  model: { provider: "test", id: "fake-model" },
  thinkingLevel: "medium",
  error: null,
} as const;

function emit(envelope: EventEnvelope) {
  act(() => mocks.eventHandler?.({ payload: envelope }));
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.open.mockReset();
  mocks.listen.mockReset();
  mocks.unlisten.mockReset();
  mocks.eventHandler = undefined;
  mocks.listen.mockImplementation(async (_name, handler) => {
    mocks.eventHandler = handler;
    return mocks.unlisten;
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("App", () => {
  it("按设计规约呈现任务导航与单一主工作区", () => {
    render(<App />);

    const navigation = screen.getByRole("navigation", { name: "任务导航" });
    expect(within(navigation).getByText("Lure")).toBeInTheDocument();
    expect(
      within(navigation).getByRole("button", { name: "新建任务" }),
    ).toBeInTheDocument();
    expect(
      within(navigation).getByRole("searchbox", { name: "搜索任务" }),
    ).toBeInTheDocument();
    expect(within(navigation).getByText("今天")).toBeInTheDocument();
    expect(within(navigation).getByText("最近")).toBeInTheDocument();
    expect(within(navigation).getByText("已归档")).toBeInTheDocument();
    expect(
      within(navigation).getByRole("button", { name: "设置" }),
    ).toBeInTheDocument();
    expect(
      within(navigation).queryByRole("button", { name: "选择工作目录" }),
    ).not.toBeInTheDocument();

    expect(
      screen.getByRole("main", { name: "任务工作区" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "开始一个新任务" }),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", {
        name: /分析当前项目|检查未提交改动|解释代码结构/,
      }),
    ).toHaveLength(3);
  });

  it("可在窄窗口打开和关闭任务导航", () => {
    render(<App />);

    const openButton = screen.getByRole("button", { name: "打开任务导航" });
    expect(openButton).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(openButton);
    expect(screen.getByRole("button", { name: "关闭任务导航" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("未连接时禁用输入，并可选择目录和连接 Pi", async () => {
    mocks.open.mockResolvedValue("/tmp/lure-project");
    mocks.invoke.mockImplementation(async (command) => {
      if (command === "connect_pi") return readySnapshot;
      return undefined;
    });
    render(<App />);

    const input = screen.getByPlaceholderText("连接 Pi 后即可发送消息…");
    expect(input).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "选择工作目录" }));
    expect(await screen.findAllByText("/tmp/lure-project")).not.toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "连接 Pi" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: "/tmp/lure-project",
      }),
    );
    expect(await screen.findByText("fake-model")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("给 Pi 发送消息…")).toBeEnabled();

    fireEvent.pointerDown(screen.getByRole("button", { name: "更多任务操作" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "断开 Pi" }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("disconnect_pi"));
    expect(
      await screen.findByPlaceholderText("连接 Pi 后即可发送消息…"),
    ).toBeDisabled();
  });

  it("发送提示词并展示 Pi 的流式响应", async () => {
    mocks.open.mockResolvedValue("/tmp/lure-project");
    mocks.invoke.mockImplementation(async (command) => {
      if (command === "connect_pi") return readySnapshot;
      if (command === "send_prompt") return { accepted: true };
      return undefined;
    });
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "选择工作目录" }));
    await screen.findAllByText("/tmp/lure-project");
    fireEvent.click(screen.getByRole("button", { name: "连接 Pi" }));
    const input = await screen.findByPlaceholderText("给 Pi 发送消息…");

    fireEvent.change(input, { target: { value: "检查项目" } });
    fireEvent.submit(input.closest("form")!);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", {
        message: "检查项目",
      }),
    );

    emit({
      sequence: 1,
      event: {
        type: "user_message_accepted",
        requestId: "2",
        message: "检查项目",
      },
    });
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: { type: "assistant_thinking_delta", contentIndex: 0, delta: "分析中" },
    });
    emit({
      sequence: 5,
      event: { type: "assistant_text_delta", contentIndex: 1, delta: "完成" },
    });
    emit({
      sequence: 6,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\n  \"path\": \"README.md\"\n}",
      },
    });

    expect(screen.getByText("检查项目")).toBeInTheDocument();
    expect(await screen.findByText("完成")).toBeInTheDocument();
    expect(screen.getByText("Thinking…")).toBeInTheDocument();
    expect(screen.getByText("read")).toBeInTheDocument();
    expect(screen.getByText("Parameters")).toBeInTheDocument();
    expect(screen.getByText(/README\.md/)).toBeInTheDocument();

    emit({
      sequence: 7,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\n  \"path\": \"README.md\"\n}",
        output: "done",
        truncatedLines: null,
        isError: false,
      },
    });
    expect(screen.getByText("Completed")).toBeInTheDocument();
    expect(screen.queryByText("Parameters")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停止生成" }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("abort_pi"));
  });

  it("以可折叠系统色块展示压缩摘要", async () => {
    render(<App />);
    await waitFor(() => expect(mocks.listen).toHaveBeenCalled());

    emit({
      sequence: 1,
      event: { type: "compaction_changed", active: true, reason: "threshold" },
    });
    expect(screen.getAllByText("Compacting context…")).not.toHaveLength(0);

    emit({
      sequence: 2,
      event: {
        type: "compaction_changed",
        active: false,
        reason: "threshold",
        aborted: false,
        summary: "保留的摘要",
        tokensBefore: 12000,
      },
    });
    expect(screen.getByText("Compacted from 12000 tokens (click to expand)")).toBeInTheDocument();
    expect(screen.getByText("[compaction]")).toBeInTheDocument();
    expect(screen.getByText("保留的摘要")).toBeInTheDocument();
  });

  it("卸载时取消 Pi 事件监听", async () => {
    const view = render(<App />);
    await waitFor(() => expect(mocks.listen).toHaveBeenCalled());

    view.unmount();

    expect(mocks.unlisten).toHaveBeenCalled();
  });
});
