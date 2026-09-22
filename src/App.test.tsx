import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { StrictMode } from "react";

import type { EventEnvelope } from "@/features/pi-connection";
import App from "./App";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
  listen: vi.fn(),
  unlisten: vi.fn(),
  createObjectURL: vi.fn(() => "blob:mock-url"),
  revokeObjectURL: vi.fn(),
  listeners: new Set<(event: { payload: EventEnvelope }) => void>(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

const defaultWorkspace = "/home/test/lure";

const readySnapshot = {
  phase: "ready",
  workingDirectory: defaultWorkspace,
  sessionId: "session-1",
  sessionFile: "/tmp/session.jsonl",
  model: { provider: "test", id: "fake-model" },
  thinkingLevel: "medium",
  error: null,
} as const;

function emit(envelope: EventEnvelope) {
  act(() => {
    for (const listener of [...mocks.listeners]) listener({ payload: envelope });
  });
}

async function renderConnected() {
  render(<App />);
  await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
    workingDirectory: defaultWorkspace,
  }));
  await screen.findByRole("combobox", { name: "模型" });
}

function textbox() {
  return screen.getByRole("textbox", { name: "任务指令" });
}

beforeEach(() => {
  localStorage.clear();
  mocks.invoke.mockReset();
  mocks.open.mockReset();
  mocks.listen.mockReset();
  mocks.unlisten.mockReset();
  mocks.createObjectURL.mockClear();
  mocks.revokeObjectURL.mockClear();
  mocks.listeners.clear();
  mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
    if (command === "get_default_workspace") return defaultWorkspace;
    if (command === "get_pi_state") {
      return {
        phase: "disconnected",
        workingDirectory: null,
        sessionId: null,
        sessionFile: null,
        model: null,
        thinkingLevel: null,
        error: null,
      };
    }
    if (command === "connect_pi") {
      return { ...readySnapshot, workingDirectory: args?.workingDirectory ?? defaultWorkspace };
    }
    if (command === "new_pi_session") {
      return {
        ...readySnapshot,
        sessionId: "session-2",
        sessionFile: "/tmp/session-2.jsonl",
      };
    }
    if (command === "send_prompt") return { accepted: true };
    if (command === "get_available_models") {
      return [
        { provider: "test", id: "fake-model" },
        { provider: "test", id: "other-model" },
        { provider: "qwen-token-plan-cn", id: "deepseek-v4.1-flash" },
      ];
    }
    if (command === "get_commands") {
      return [{ name: "review", description: "审查改动", source: "extension" }];
    }
    if (command === "list_project_sessions") {
      const directory = args?.workingDirectory;
      if (directory === "/tmp/lure-project") {
        return { hasMore: false, sessions: [
          {
            path: "/tmp/lure-project/sessions/new.jsonl",
            id: "session-project",
            cwd: "/tmp/lure-project",
            name: null,
            parentSessionPath: null,
            createdAtMs: 1_700_100_000_000,
            modifiedAtMs: 1_700_100_000_000,
            messageCount: 1,
            firstMessage: "项目里的对话",
          },
        ] };
      }
      return { hasMore: false, sessions: [
        {
          path: "/tmp/lure/sessions/old.jsonl",
          id: "session-old",
          cwd: defaultWorkspace,
          name: null,
          parentSessionPath: null,
          createdAtMs: 1_700_000_000_000,
          modifiedAtMs: 1_700_000_000_000,
          messageCount: 2,
          firstMessage: "昨天的工作",
        },
      ] };
    }
    if (command === "switch_pi_session") {
      return {
        switched: true,
        snapshot: {
          ...readySnapshot,
          sessionFile: "/tmp/lure/sessions/old.jsonl",
          sessionId: "session-old",
        },
      };
    }
    if (command === "get_session_entries") {
      return {
        entries: [
          { type: "message", id: "e1", parentId: null, message: { role: "user", content: "历史提问" } },
          {
            type: "message",
            id: "e2",
            parentId: "e1",
            message: { role: "assistant", content: [{ type: "text", text: "历史回复" }] },
          },
        ],
        leafId: "e2",
      };
    }
    if (command === "get_workspace_context") {
      return { workingDirectory: defaultWorkspace, branch: "main" };
    }
    if (command === "set_model") return { provider: "test", id: "other-model" };
    if (command === "set_thinking_level") return "high";
    if (command === "read_image_attachments") {
      return [{ data: "aGVsbG8=", mimeType: "image/png", name: "shot.png" }];
    }
    return undefined;
  });
  mocks.open.mockImplementation(async (options: { directory?: boolean } | undefined) =>
    options?.directory ? "/tmp/lure-project" : ["/tmp/shot.png"],
  );
  mocks.listen.mockImplementation(async (_name, handler) => {
    mocks.listeners.add(handler);
    return () => {
      mocks.listeners.delete(handler);
      mocks.unlisten();
    };
  });
  Object.assign(URL, {
    createObjectURL: mocks.createObjectURL,
    revokeObjectURL: mocks.revokeObjectURL,
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("主工作区初始态", () => {
  it("启动后准备默认工作区并自动创建 Pi RPC", async () => {
    render(<App />);

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("get_default_workspace"),
    );
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: defaultWorkspace,
      }),
    );
    expect(mocks.open).not.toHaveBeenCalled();
  });

  it("连接 Pi 时展示加载反馈，并在连接完成后移除", () => {
    render(<App />);

    emit({
      sequence: 1,
      event: {
        type: "connection_changed",
        snapshot: { ...readySnapshot, phase: "connecting", model: null, thinkingLevel: null },
      },
    });
    expect(screen.getByRole("status", { name: "正在启动 Pi" })).toHaveTextContent(
      "正在加载本地工具",
    );

    emit({ sequence: 2, event: { type: "connection_changed", snapshot: readySnapshot } });
    expect(screen.queryByRole("status", { name: "正在启动 Pi" })).not.toBeInTheDocument();
  });

  it("不显示会话顶栏，并在底部提供悬浮输入卡", () => {
    render(<App />);

    expect(screen.queryByLabelText("任务顶栏")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "开始一个新任务" })).toBeInTheDocument();

    const card = screen.getByRole("group", { name: "任务输入卡" });
    expect(card.className).toContain("rounded-[20px]");
    expect(card.className).not.toContain("border-t");

    const controls = within(card).getByRole("group", { name: "输入控制栏" });
    expect(controls.className).toContain("flex-nowrap");
    expect(within(controls).getByRole("button", { name: "发送消息" }).parentElement?.className).toContain(
      "ml-auto",
    );

    const content = screen.getByRole("log", { name: "对话线程" });
    expect(content.className).toContain("w-full");
    expect(content.className).not.toContain("max-w-[720px]");
    expect(card.className).toContain("max-w-[720px]");

    const viewport = screen.getByLabelText("对话滚动区");
    expect(viewport.className).toContain("overflow-y-auto");
    expect(viewport.className).toContain("overflow-x-hidden");
    expect(viewport.className).toContain("overscroll-none");
    expect(viewport.className).toContain("flex-col");
    expect(within(viewport).getByRole("group", { name: "任务输入卡" })).toBe(card);

    const conversationColumn = screen.getByLabelText("对话内容列");
    expect(conversationColumn.className).toContain("flex-1");
    expect(conversationColumn.className).not.toContain("min-h-full");
  });

  it("固定默认工作目录，不提供选择或更换目录操作", () => {
    render(<App />);

    expect(screen.queryByRole("button", { name: "更换目录" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "选择工作目录" })).not.toBeInTheDocument();
    expect(screen.queryByText("更换工作目录")).not.toBeInTheDocument();
  });

  it("未连接时可编辑草稿但禁止发送", async () => {
    render(<App />);

    expect(textbox()).toBeEnabled();
    expect(screen.getByRole("button", { name: "发送消息" })).toBeDisabled();

    fireEvent.change(textbox(), { target: { value: "准备中的草稿" } });
    expect(textbox()).toHaveValue("准备中的草稿");
    expect(screen.getByRole("button", { name: "发送消息" })).toBeDisabled();
  });

  it("工作流示例只填入草稿且不自动执行", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "分析当前项目" }));

    await waitFor(() => expect(textbox()).toHaveValue("分析当前项目"));
    expect(screen.queryByText("分析当前项目", { selector: "p, div, h2" })).toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalledWith("send_prompt", expect.anything());
  });

  it("意外关闭后恢复默认工作目录中的未发送草稿", async () => {
    const view = render(<App />);
    await screen.findByRole("combobox", { name: "模型" });
    fireEvent.change(textbox(), { target: { value: "还没发送的草稿" } });
    view.unmount();

    render(<App />);

    await screen.findByRole("combobox", { name: "模型" });
    expect(textbox()).toHaveValue("还没发送的草稿");
  });

  it("取消图片确认时不会选择文件", async () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "添加图片" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));

    expect(mocks.open).not.toHaveBeenCalled();
    expect(screen.queryByText("shot.png")).not.toBeInTheDocument();
  });

  it("窄窗口可以打开任务导航", () => {
    render(<App />);

    const open = screen.getByRole("button", { name: "打开任务导航" });
    expect(open).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(open);
    expect(screen.getByRole("button", { name: "关闭任务导航" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("任务导航展示最近与项目分组，不显示搜索或已归档分组", () => {
    render(<App />);

    expect(screen.getByRole("button", { name: "新建任务" })).toBeInTheDocument();
    expect(screen.queryByRole("searchbox", { name: "搜索任务" })).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "最近" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "项目" })).toBeInTheDocument();
    expect(screen.queryByText("已归档")).not.toBeInTheDocument();
  });

  it("任务导航采用紧凑的浅灰布局", () => {
    render(<App />);

    expect(screen.getByRole("navigation", { name: "任务导航" })).toHaveClass("bg-[#F6F6F8]");
    expect(screen.getByText("Lure").parentElement).toHaveClass("h-14");
    expect(screen.getByRole("button", { name: "新建任务" })).toHaveClass("h-11", "px-5");
  });

  it("默认工作目录只显示在最近中，并可通过弹框添加本地项目", async () => {
    await renderConnected();

    expect(screen.getByRole("heading", { name: "项目" })).toBeInTheDocument();
    const recent = screen.getByRole("region", { name: "最近" });
    expect(within(recent).getByText("昨天的工作")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "在 lure 中新建任务" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "新增项目" }));
    const dialog = screen.getByRole("dialog", { name: "创建项目" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择 Pi 可读取和编辑的文件夹" }));

    await waitFor(() =>
      expect(mocks.open).toHaveBeenCalledWith({ directory: true, multiple: false }),
    );
    expect(within(dialog).getByRole("textbox", { name: "项目名称" })).toHaveValue("lure-project");
    fireEvent.click(within(dialog).getByRole("button", { name: "创建项目" }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: "/tmp/lure-project",
      }),
    );
    expect(screen.getByRole("button", { name: "在 lure-project 中新建任务" })).toBeInTheDocument();
    expect(JSON.parse(localStorage.getItem("lure:projects") ?? "[]")).toEqual([
      { directory: "/tmp/lure-project", name: "lure-project" },
    ]);
  });
});

describe("主工作区对话态", () => {
  it("新建任务复用默认 RPC client，并清空旧对话后聚焦输入框", async () => {
    await renderConnected();
    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "old", message: "旧对话" },
    });
    expect(screen.getByText("旧对话")).toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "任务标题" }), {
      target: { value: "旧任务" },
    });

    fireEvent.click(screen.getByRole("button", { name: "新建任务" }));

    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("new_pi_session"));
    expect(mocks.invoke).not.toHaveBeenCalledWith("disconnect_pi");
    expect(mocks.open).not.toHaveBeenCalled();
    expect(screen.queryByText("旧对话")).not.toBeInTheDocument();
    // 历史会话来自 Pi 记录的会话文件，而不是本地记忆。
    expect(
      within(screen.getByRole("region", { name: "最近" })).getByText("昨天的工作"),
    ).toBeInTheDocument();
    expect(textbox()).toHaveFocus();
  });

  it("从其他项目点击顶部新建任务时回到默认 lure 项目", async () => {
    await renderConnected();
    fireEvent.click(screen.getByRole("button", { name: "新增项目" }));
    const dialog = screen.getByRole("dialog", { name: "创建项目" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择 Pi 可读取和编辑的文件夹" }));
    await waitFor(() =>
      expect(within(dialog).getByRole("textbox", { name: "项目名称" })).toHaveValue("lure-project"),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "创建项目" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: "/tmp/lure-project",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: "新建任务" }));

    await waitFor(() => {
      const defaultConnections = mocks.invoke.mock.calls.filter(
        ([command, args]) =>
          command === "connect_pi" && args?.workingDirectory === defaultWorkspace,
      );
      expect(defaultConnections).toHaveLength(2);
    });
  });

  it("按 Ctrl+L 打开模型选择框，并在确认后切换模型", async () => {
    await renderConnected();

    fireEvent.keyDown(textbox(), { ctrlKey: true, key: "l" });

    const dialog = await screen.findByRole("dialog", { name: "选择模型" });
    expect(within(dialog).getByRole("tab", { name: "test" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.click(within(dialog).getByRole("tab", { name: "qwen-token-plan-cn" }));
    fireEvent.click(
      within(dialog).getByRole("radio", { name: "deepseek-v4.1-flash" }),
    );
    expect(mocks.invoke).not.toHaveBeenCalledWith("set_model", expect.anything());

    fireEvent.click(within(dialog).getByRole("button", { name: "确认选择" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("set_model", {
        modelId: "deepseek-v4.1-flash",
        provider: "qwen-token-plan-cn",
      }),
    );
  });

  it("连接后移除输入卡中重复的目录与分支条带", async () => {
    await renderConnected();

    const card = screen.getByRole("group", { name: "任务输入卡" });
    expect(within(card).queryByText("main")).not.toBeInTheDocument();
    expect(within(card).queryByText("lure")).not.toBeInTheDocument();
    expect(within(card).queryByText("已连接")).not.toBeInTheDocument();
    const modelSelect = within(card).getByRole("combobox", { name: "模型" });
    expect(modelSelect.className).toContain("truncate");
    expect(modelSelect).toBeInTheDocument();
    const testProvider = within(modelSelect).getByRole("group", { name: "test" });
    const qwenProvider = within(modelSelect).getByRole("group", {
      name: "qwen-token-plan-cn",
    });
    expect(within(testProvider).getByRole("option", { name: "fake-model [test]" })).toBeInTheDocument();
    expect(within(testProvider).getByRole("option", { name: "other-model [test]" })).toBeInTheDocument();
    expect(
      within(qwenProvider).getByRole("option", {
        name: "deepseek-v4.1-flash [qwen-token-plan-cn]",
      }),
    ).toBeInTheDocument();
    expect(within(card).getByRole("combobox", { name: "思考强度" })).toBeInTheDocument();

    fireEvent.change(modelSelect, {
      target: { value: "test::other-model" },
    });
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("set_model", {
        modelId: "other-model",
        provider: "test",
      }),
    );
  });

  it("发送后出现顶栏和用户消息，运行生命周期不进入对话流", async () => {
    await renderConnected();

    fireEvent.change(textbox(), { target: { value: "检查项目" } });
    expect(screen.getByRole("button", { name: "发送消息" })).toBeEnabled();
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", {
        images: [],
        message: "检查项目",
      }),
    );

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "2", message: "检查项目" },
    });
    expect(await screen.findByText("检查项目")).toBeInTheDocument();

    const header = screen.getByLabelText("任务顶栏");
    expect(within(header).getByRole("textbox", { name: "任务标题" })).toHaveValue("lure");
    expect(within(header).getByRole("button", { name: "导出记录" })).toBeInTheDocument();
    expect(within(header).getByRole("button", { name: "更多任务操作" })).toBeInTheDocument();
    expect(screen.queryByText("更换工作目录")).not.toBeInTheDocument();
    expect(within(header).queryByText("执行中")).not.toBeInTheDocument();
    expect(within(header).queryByText("fake-model")).not.toBeInTheDocument();

    emit({ sequence: 2, event: { type: "run_started" } });
    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    expect(screen.queryByText("Pi 已开始执行任务")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止生成" })).toBeInTheDocument();
  });

  it("保留 Pi 产生的连续助手轮次及其工具归属", async () => {
    await renderConnected();

    emit({ sequence: 1, event: { type: "assistant_message_started" } });
    emit({
      sequence: 2,
      event: {
        type: "assistant_message_completed",
        text: "先读取文件",
        thinking: "",
      },
    });
    emit({
      sequence: 3,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"README.md\"}",
      },
    });
    emit({ sequence: 4, event: { type: "assistant_message_started" } });
    emit({
      sequence: 5,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "读取完成" },
    });

    expect(screen.getAllByLabelText("Pi 回复")).toHaveLength(2);
    expect(screen.getByText("先读取文件")).toBeInTheDocument();
    expect(await screen.findByText("读取完成")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /正在读取 README.md/ })).toBeInTheDocument();
  });

  it("中文组合输入不写入拼音中间态，提交后保留完整中文", async () => {
    await renderConnected();
    const field = textbox() as HTMLTextAreaElement;
    const draftKey = `lure:draft:${defaultWorkspace}`;

    // 拼音组合过程：composer 自己维护受控值，外部不得把中间态当作草稿写回。
    fireEvent.compositionStart(field);
    fireEvent.change(field, { target: { value: "j" } });
    fireEvent.change(field, { target: { value: "jin" } });
    expect(localStorage.getItem(draftKey)).toBeNull();
    expect(screen.getByRole("button", { name: "发送消息" })).toBeDisabled();

    // IME 提交：只写入完整中文，不出现拼音累积。
    field.value = "今天";
    fireEvent.compositionEnd(field);
    fireEvent.change(field, { target: { value: "今天" } });
    expect(localStorage.getItem(draftKey)).toBe("今天");
    expect(field).toHaveValue("今天");
    expect(screen.getByRole("button", { name: "发送消息" })).toBeEnabled();
  });

  it("工具卡按真实顺序渲染在两条助手文本之间", async () => {
    await renderConnected();

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "1", message: "检查项目" },
    });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "先读取配置" },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"package.json\"}",
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"package.json\"}",
        output: "{\"name\":\"lure\"}",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({
      sequence: 6,
      event: { type: "assistant_text_delta", contentIndex: 2, delta: "再看入口" },
    });

    await screen.findByText("再看入口");
    const bubble = await screen.findByLabelText("Pi 回复");
    const rendered = bubble.textContent ?? "";
    const before = rendered.indexOf("先读取配置");
    const tool = rendered.indexOf("已读取 package.json");
    const after = rendered.indexOf("再看入口");

    expect(before).toBeGreaterThanOrEqual(0);
    expect(tool).toBeGreaterThan(before);
    expect(after).toBeGreaterThan(tool);
  });

  it("同一回复中已结束与进行中的工具分别显示各自状态", async () => {
    await renderConnected();

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "1", message: "检查项目" },
    });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"package.json\"}",
      },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"package.json\"}",
        output: "{\"name\":\"lure\"}",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_started",
        toolCallId: "tool-2",
        toolName: "read",
        input: "{\"path\":\"src/main.tsx\"}",
      },
    });

    await screen.findByLabelText("Pi 回复");

    expect(
      await screen.findByRole("button", { name: /已读取 package.json/ }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /正在读取 src\/main.tsx/ }),
    ).toBeInTheDocument();
  });

  it("运行中保留可编辑草稿并把主操作切换为停止", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "第一条" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    // 运行开始不应把焦点从用户手上拿走：用户可能正在编辑下一条指令或阅读执行过程。
    const imageButton = screen.getByRole("button", { name: "添加图片" });
    imageButton.focus();
    emit({ sequence: 1, event: { type: "run_started" } });
    expect(imageButton).toHaveFocus();
    expect(textbox()).not.toHaveFocus();

    expect(textbox()).toBeEnabled();
    fireEvent.change(textbox(), { target: { value: "下一条草稿" } });
    expect(textbox()).toHaveValue("下一条草稿");
    expect(screen.queryByRole("button", { name: "发送消息" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停止生成" }));
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("abort_pi"));
    expect(textbox()).toHaveValue("下一条草稿");
  });

  it("settled 后保留真实回复和工具卡片，不生成完成状态消息", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "修改文件" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "已经改好" },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "edit",
        input: "{\n  \"path\": \"src/App.tsx\"\n}",
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "edit",
        input: "{\n  \"path\": \"src/App.tsx\"\n}",
        output: "updated src/App.tsx",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({ sequence: 6, event: { type: "run_settled" } });

    expect(await screen.findByText("已经改好")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /已修改 src\/App.tsx/ })).toBeInTheDocument();
    expect(screen.queryByRole("article", { name: /结果：/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    expect(screen.queryByText("任务执行完成")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "发送消息" })).toBeInTheDocument();
  });

  it("用户停止只在对应助手轮次说明，不生成生命周期消息", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "长任务" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: {
        type: "assistant_message_completed",
        text: "",
        thinking: "",
        stopReason: "aborted",
      },
    });
    emit({ sequence: 4, event: { type: "run_settled" } });

    expect(screen.getByText("任务已由你停止，已完成的内容仍然保留。")).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    expect(screen.queryByText("任务执行完成")).not.toBeInTheDocument();
  });
});

describe("历史会话", () => {
  it("从最近打开历史会话并显示 Pi 记录的历史对话", async () => {
    await renderConnected();

    const recent = screen.getByRole("region", { name: "最近" });
    fireEvent.click(within(recent).getByRole("button", { name: /昨天的工作/ }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("switch_pi_session", {
        sessionPath: "/tmp/lure/sessions/old.jsonl",
      }),
    );
    expect(await screen.findByText("历史提问")).toBeInTheDocument();
    expect(screen.getByText("历史回复")).toBeInTheDocument();
  });

  it("打开的历史会话在最近中显示为当前任务", async () => {
    await renderConnected();

    const recent = screen.getByRole("region", { name: "最近" });
    fireEvent.click(within(recent).getByRole("button", { name: /昨天的工作/ }));

    const current = await within(recent).findByRole("button", { name: /昨天的工作/ });
    await waitFor(() => expect(current).toHaveAttribute("aria-current", "page"));
    expect(within(current).getByText("已连接")).toBeInTheDocument();
  });

  it("点击项目展开并加载该项目的历史会话，再次点击折叠", async () => {
    await renderConnected();
    fireEvent.click(screen.getByRole("button", { name: "新增项目" }));
    const dialog = screen.getByRole("dialog", { name: "创建项目" });
    fireEvent.click(within(dialog).getByRole("button", { name: "选择 Pi 可读取和编辑的文件夹" }));
    await waitFor(() =>
      expect(within(dialog).getByRole("textbox", { name: "项目名称" })).toHaveValue("lure-project"),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "创建项目" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: "/tmp/lure-project",
      }),
    );

    // “最近”始终是默认工作目录的历史会话，与当前所在项目无关。
    const recent = screen.getByRole("region", { name: "最近" });
    expect(within(recent).getByText("昨天的工作")).toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "lure-project 历史会话" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("项目里的对话")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    await waitFor(() => expect(toggle).toHaveAttribute("aria-expanded", "true"));
    expect(await screen.findByText("项目里的对话")).toBeInTheDocument();
    expect(within(recent).getByText("昨天的工作")).toBeInTheDocument();
    expect(within(recent).queryByText("项目里的对话")).not.toBeInTheDocument();

    fireEvent.click(toggle);

    await waitFor(() => expect(toggle).toHaveAttribute("aria-expanded", "false"));
    expect(screen.queryByText("项目里的对话")).not.toBeInTheDocument();
  });

  it("运行中不允许切换历史会话", async () => {
    await renderConnected();
    emit({ sequence: 1, event: { type: "run_started" } });

    const recent = screen.getByRole("region", { name: "最近" });
    expect(within(recent).getByRole("button", { name: /昨天的工作/ })).toBeDisabled();
  });
});

describe("执行流渐进展开", () => {
  it("工具默认折叠，展开后显示中文字段与完整输出入口", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "读取" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "bash",
        input: "{\n  \"command\": \"pnpm test\"\n}",
      },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "bash",
        input: "{\n  \"command\": \"pnpm test\"\n}",
        output: Array.from({ length: 30 }, (_, index) => `line ${index}`).join("\n"),
        truncatedLines: null,
        isError: false,
      },
    });

    const summary = screen.getByRole("button", { name: /项目命令执行完成/ });
    expect(summary).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("工具参数")).not.toBeInTheDocument();

    fireEvent.click(summary);
    expect(summary).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("工具参数")).toBeInTheDocument();
    expect(screen.getByText("工具输出")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /查看完整 30 行输出/ })).toBeInTheDocument();
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.queryByText("Completed")).not.toBeInTheDocument();
  });
});

describe("完整内容预览", () => {
  it("长工具输出在工具卡内打开预览并恢复触发点焦点", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "读取配置" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    const output = Array.from({ length: 12 }, (_, index) => `line ${index + 1}`).join("\n");

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "bash",
        input: "{\n  \"command\": \"pnpm test\"\n}",
      },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "bash",
        input: "{\n  \"command\": \"pnpm test\"\n}",
        output,
        truncatedLines: null,
        isError: false,
      },
    });

    fireEvent.click(await screen.findByRole("button", { name: /项目命令执行完成/ }));
    const openPreview = screen.getByRole("button", { name: "查看完整 12 行输出" });
    openPreview.focus();
    fireEvent.click(openPreview);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("项目命令执行完成")).toBeInTheDocument();
    expect(within(dialog).getByText(/line 12/)).toBeInTheDocument();

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(openPreview).toHaveFocus();
  });

  it("展开一个工具调用不影响其他工具", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "两个工具" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\n  \"path\": \"src/App.tsx\"\n}",
      },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_started",
        toolCallId: "tool-2",
        toolName: "bash",
        input: "{\n  \"command\": \"git status\"\n}",
      },
    });

    const first = screen.getByRole("button", { name: /正在读取 src\/App.tsx/ });
    const second = screen.getByRole("button", { name: /正在执行项目命令/ });
    expect(first).toHaveAttribute("aria-expanded", "false");
    expect(second).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(first);

    expect(first).toHaveAttribute("aria-expanded", "true");
    expect(second).toHaveAttribute("aria-expanded", "false");
  });
});

describe("风险与错误处理", () => {
  it("扩展交互请求打开原生对话框并把响应回传 Pi", async () => {
    await renderConnected();

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({
      sequence: 2,
      event: {
        type: "extension_ui_requested",
        requestId: "ui-1",
        method: "select",
        title: "选择范围",
        message: "请选择处理范围",
        options: ["当前文件", "整个项目"],
        placeholder: null,
        defaultValue: null,
      },
    });

    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("选择范围")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "取消" })).toBeInTheDocument();

    fireEvent.change(screen.getByRole("combobox", { name: "选择一项" }), {
      target: { value: "整个项目" },
    });
    fireEvent.click(screen.getByRole("button", { name: "提交输入" }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("respond_extension_ui", {
        cancelled: false,
        requestId: "ui-1",
        value: "整个项目",
      }),
    );
  });

  it("失败时说明影响与原因，并提供可执行操作", async () => {
    await renderConnected();

    emit({ sequence: 1, event: { type: "process_exited", code: 1 } });

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("影响：");
    expect(alert.textContent).toContain("原因：");
    expect(within(alert).getByRole("button", { name: "重新连接" })).toBeInTheDocument();
  });

  it("导出记录把当前对话写成 Markdown", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "导出我" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));
    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "5", message: "导出我" },
    });

    fireEvent.click(screen.getByRole("button", { name: "导出记录" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("操作内容")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "导出并保存" }));

    expect(mocks.createObjectURL).toHaveBeenCalledTimes(1);
    expect(mocks.revokeObjectURL).toHaveBeenCalledTimes(1);
  });
});

describe("附件", () => {
  it("选择图片后显示附件并可随指令发送", async () => {
    await renderConnected();

    fireEvent.click(screen.getByRole("button", { name: "添加图片" }));
    const confirmDialog = await screen.findByRole("dialog");
    expect(within(confirmDialog).getByText("影响对象")).toBeInTheDocument();
    expect(within(confirmDialog).getByText("可否恢复")).toBeInTheDocument();
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "继续选择图片" }));
    expect(await screen.findByText("shot.png")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "移除附件 shot.png" }));
    await waitFor(() => expect(screen.queryByText("shot.png")).not.toBeInTheDocument());

    fireEvent.click(screen.getByRole("button", { name: "添加图片" }));
    fireEvent.click(await screen.findByRole("button", { name: "继续选择图片" }));
    expect(await screen.findByText("shot.png")).toBeInTheDocument();

    fireEvent.change(textbox(), { target: { value: "看看这张图" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", {
        images: [{ data: "aGVsbG8=", mimeType: "image/png" }],
        message: "看看这张图",
      }),
    );

    await waitFor(() => expect(screen.queryByText("shot.png")).not.toBeInTheDocument());
  });

  it("允许只发送图片附件", async () => {
    await renderConnected();

    fireEvent.click(screen.getByRole("button", { name: "添加图片" }));
    fireEvent.click(await screen.findByRole("button", { name: "继续选择图片" }));
    expect(await screen.findByText("shot.png")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "发送消息" })).toBeEnabled();

    fireEvent.submit(textbox().closest("form")!);

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", {
        images: [{ data: "aGVsbG8=", mimeType: "image/png" }],
        message: "",
      }),
    );
  });
});
describe("事件订阅生命周期", () => {
  it("严格模式重复挂载后只保留一个 Pi 事件监听", async () => {
    render(
      <StrictMode>
        <App />
      </StrictMode>,
    );

    await waitFor(() => expect(mocks.listen).toHaveBeenCalled());
    await waitFor(() => expect(mocks.listeners.size).toBe(1));
    await waitFor(() =>
      expect(mocks.invoke.mock.calls.filter(([command]) => command === "connect_pi")).toHaveLength(1),
    );

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "1", message: "只出现一次" },
    });

    expect(screen.getAllByText("只出现一次")).toHaveLength(1);
  });

  it("重复投递的同一事件只应用一次", async () => {
    const view = render(<App />);
    await waitFor(() => expect(mocks.listen).toHaveBeenCalled());

    const listener = [...mocks.listeners][0];
    act(() => {
      // 模拟两个活跃监听同时收到同一个 RPC 事件。
      listener?.({ payload: { sequence: 7, event: { type: "run_started" } } });
      listener?.({ payload: { sequence: 7, event: { type: "run_started" } } });
    });

    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    expect(screen.queryByText("Pi 已开始执行任务")).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "停止生成" })).toHaveLength(1);
    view.unmount();
  });

  it("卸载时取消 Pi 事件监听", async () => {
    const view = render(<App />);
    await waitFor(() => expect(mocks.listen).toHaveBeenCalled());

    view.unmount();

    expect(mocks.unlisten).toHaveBeenCalled();
    expect(mocks.listeners.size).toBe(0);
  });
});
