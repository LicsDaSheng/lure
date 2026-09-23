import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { StrictMode } from "react";

import type { EventEnvelope } from "@/lib/pi-rpc/types";
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


function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
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
  it("使用圆角裁切桌面窗口外框", () => {
    render(<App />);

    const frame = screen.getByTestId("app-frame");
    expect(frame.className).toContain("rounded-2xl");
    expect(frame.className).toContain("overflow-hidden");
    expect(frame.className).toContain("border");
  });

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

  it("连接过程在后台静默进行，不出现等待或连接状态提示", () => {
    render(<App />);

    emit({
      sequence: 1,
      event: {
        type: "connection_changed",
        snapshot: { ...readySnapshot, phase: "connecting", model: null, thinkingLevel: null },
      },
    });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(screen.queryByText(/正在启动|正在连接|正在打开|正在载入/)).not.toBeInTheDocument();

    emit({ sequence: 2, event: { type: "connection_changed", snapshot: readySnapshot } });
    expect(screen.queryByText(/已连接|正在连接|未连接/)).not.toBeInTheDocument();
  });

  it("连接失败时弹窗说明影响与原因，并提供重新连接", async () => {
    const baseInvoke = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "connect_pi") throw new Error("pi 进程启动失败");
      return baseInvoke(command, args);
    });
    render(<App />);

    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("影响：");
    expect(dialog.textContent).toContain("原因：");
    expect(within(dialog).getByRole("button", { name: "重新连接" })).toBeInTheDocument();
    // 连接失败不以内联面板重复提示。
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
    expect(conversationColumn.className).toContain("max-w-[768px]");
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

    expect(screen.getByRole("navigation", { name: "任务导航" })).toHaveClass("bg-sidebar");
    expect(screen.getByRole("banner", { name: "应用标题栏" })).toHaveClass("h-12");
    expect(screen.getByRole("button", { name: "新建任务" })).toHaveClass("h-11", "px-5");
  });

  it("标题栏与左右两列保持同色，并可折叠左侧导航", () => {
    render(<App />);

    const titleBar = screen.getByRole("banner", { name: "应用标题栏" });
    expect(titleBar.firstElementChild).toHaveClass("bg-sidebar", "border-r");
    expect(titleBar.lastElementChild).toHaveClass("bg-background");

    fireEvent.click(screen.getByRole("button", { name: "折叠左侧栏" }));
    expect(screen.queryByRole("navigation", { name: "任务导航" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "展开左侧栏" }));
    expect(screen.getByRole("navigation", { name: "任务导航" })).toBeInTheDocument();
  });

  it("设置页只保留返回应用和外观，并可切换深色主题", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: "设置" }));

    expect(screen.getByRole("button", { name: "返回应用" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "设置选项" })).toHaveTextContent("外观");
    expect(screen.queryByText("Agent")).not.toBeInTheDocument();
    expect(screen.queryByText("工具")).not.toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();

    const themes = screen.getByRole("radiogroup", { name: "主题" });
    expect(within(themes).getByRole("radio", { name: "系统" })).toHaveAttribute("aria-checked", "true");
    fireEvent.click(within(themes).getByRole("radio", { name: "深色" }));

    expect(document.documentElement).toHaveClass("dark");
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    expect(localStorage.getItem("lure.theme")).toBe("dark");

    fireEvent.click(screen.getByRole("button", { name: "返回应用" }));
    expect(screen.getByRole("navigation", { name: "任务导航" })).toBeInTheDocument();
    expect(screen.queryByRole("main", { name: "外观设置" })).not.toBeInTheDocument();
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

  it("输入卡始终显示当前工作目录与分支，但不提供手动连接操作", async () => {
    await renderConnected();

    const card = screen.getByRole("group", { name: "任务输入卡" });
    expect(within(card).getByText("main")).toBeInTheDocument();
    expect(within(card).getByText("lure")).toBeInTheDocument();
    expect(within(card).queryByRole("button", { name: "连接 Pi" })).not.toBeInTheDocument();
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
    // 顶栏只承载标题与导航入口：不再提供导出、断开或更多操作。
    expect(within(header).queryByRole("button", { name: "导出记录" })).not.toBeInTheDocument();
    expect(within(header).queryByRole("button", { name: "更多任务操作" })).not.toBeInTheDocument();
    expect(within(header).queryByRole("button", { name: "断开 Pi" })).not.toBeInTheDocument();
    expect(screen.queryByText("更换工作目录")).not.toBeInTheDocument();
    expect(within(header).queryByText("执行中")).not.toBeInTheDocument();
    expect(within(header).queryByText("fake-model")).not.toBeInTheDocument();

    emit({ sequence: 2, event: { type: "run_started" } });
    expect(screen.queryByRole("status", { name: "任务状态" })).not.toBeInTheDocument();
    expect(screen.queryByText("Pi 已开始执行任务")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止生成" })).toBeInTheDocument();
  });

  it("把一次用户指令下的多条助手消息归入同一响应组", async () => {
    await renderConnected();

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "1", message: "检查项目" },
    });
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: {
        type: "assistant_message_completed",
        text: "先读取文件",
        thinking: "",
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"README.md\"}",
      },
    });
    emit({ sequence: 6, event: { type: "assistant_message_started" } });
    emit({
      sequence: 7,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "读取完成" },
    });

    // 同一次用户指令只形成一个响应组，运行中的执行过程默认展开。
    expect(screen.getAllByLabelText("Pi 回复")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "执行中" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
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
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "先读取配置" },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\":\"package.json\"}",
      },
    });
    emit({
      sequence: 6,
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
      sequence: 7,
      event: { type: "assistant_text_delta", contentIndex: 2, delta: "再看入口" },
    });

    await screen.findByText("再看入口");
    const bubble = await screen.findByLabelText("Pi 回复");
    const rendered = bubble.textContent ?? "";
    const before = rendered.indexOf("先读取配置");
    const tool = rendered.indexOf("工具调用");
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
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
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

  it("完成后执行过程默认折叠，最终结果与工具卡都在同一响应组内", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "修改文件" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({ sequence: 1, event: { type: "run_started" } });
    emit({ sequence: 2, event: { type: "assistant_message_started" } });
    emit({
      sequence: 3,
      event: { type: "assistant_text_delta", contentIndex: 0, delta: "先修改文件" },
    });
    emit({
      sequence: 4,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "edit",
        input: "{\"path\": \"src/App.tsx\"}",
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "edit",
        input: "{\"path\": \"src/App.tsx\"}",
        output: "updated src/App.tsx",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({
      sequence: 6,
      event: {
        type: "assistant_message_completed",
        text: "先修改文件",
        thinking: "",
        stopReason: "toolUse",
      },
    });
    emit({ sequence: 7, event: { type: "assistant_message_started" } });
    emit({
      sequence: 8,
      event: {
        type: "assistant_message_completed",
        text: "已经改好",
        thinking: "",
        stopReason: "stop",
      },
    });
    emit({ sequence: 9, event: { type: "run_finished", willRetry: false } });
    emit({ sequence: 10, event: { type: "run_settled" } });

    // 最终结果始终在折叠区之外，执行过程默认收起。
    expect(await screen.findByText("已经改好")).toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: /展开执行过程/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: /已修改 src\/App.tsx/ })).not.toBeInTheDocument();
    expect(screen.getByText("先修改文件")).not.toBeVisible();

    // 展开后才显示过程内容，工具卡保持原有渐进展开。
    fireEvent.click(toggle);
    expect(screen.getByRole("button", { name: /已修改 src\/App.tsx/ })).toBeInTheDocument();
    expect(screen.getByText("先修改文件")).toBeVisible();

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
    expect(within(current).queryByText(/已连接|正在连接|未连接|连接失败/)).not.toBeInTheDocument();
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

  it("打开较早的历史会话后，项目导航保留该会话而不是项目名条目", async () => {
    const projectSessions = [1, 2, 3, 4].map((index) => ({
      path: `/tmp/lure-project/sessions/${index}.jsonl`,
      id: `session-project-${index}`,
      cwd: "/tmp/lure-project",
      name: null,
      parentSessionPath: null,
      createdAtMs: 1_700_100_000_000 - index * 1_000,
      modifiedAtMs: 1_700_100_000_000 - index * 1_000,
      messageCount: 1,
      firstMessage: index === 4 ? "很早的对话" : `项目会话 ${index}`,
    }));
    const baseInvoke = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "list_project_sessions") {
        const offset = Number(args?.offset ?? 0);
        const limit = Number(args?.limit ?? 3);
        return {
          hasMore: offset + limit < projectSessions.length,
          sessions: projectSessions.slice(offset, offset + limit),
        };
      }
      if (command === "switch_pi_session") {
        const sessionPath = String(args?.sessionPath ?? "");
        const index = sessionPath.match(/(\d+)\.jsonl$/)?.[1] ?? "1";
        return {
          switched: true,
          snapshot: {
            ...readySnapshot,
            workingDirectory: "/tmp/lure-project",
            sessionFile: sessionPath,
            sessionId: `session-project-${index}`,
          },
        };
      }
      return baseInvoke(command, args);
    });

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

    fireEvent.click(screen.getByRole("button", { name: "lure-project 历史会话" }));
    const project = await screen.findByRole("region", { name: "lure-project历史会话列表" });
    await within(project).findByText("项目会话 1");

    // 加载第 4 条（最旧）会话并打开它：它不会落在刷新后的第一页里。
    fireEvent.click(within(project).getByRole("button", { name: "显示更多" }));
    fireEvent.click(await within(project).findByRole("button", { name: /很早的对话/ }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("switch_pi_session", {
        sessionPath: "/tmp/lure-project/sessions/4.jsonl",
      }),
    );
    // 打开的会话保持可见且为选中态，不再换成一条以项目名为标题的条目。
    await waitFor(() =>
      expect(within(project).getByRole("button", { name: /很早的对话/ })).toHaveAttribute(
        "aria-current",
        "page",
      ),
    );
    expect(within(project).queryByRole("button", { name: /^lure-project$/ })).not.toBeInTheDocument();
    expect(within(project).getByText("项目会话 1")).toBeInTheDocument();
  });

  it("切换完成后迟到的连接快照不会让项目分组闪现兜底条目", async () => {
    const testSession = {
      path: "/tmp/test/sessions/target.jsonl",
      id: "session-target",
      cwd: "/tmp/test",
      name: null,
      parentSessionPath: null,
      createdAtMs: 1_700_200_000_000,
      modifiedAtMs: 1_700_200_000_000,
      messageCount: 2,
      firstMessage: "测试会话",
    };
    localStorage.setItem(
      "lure:projects",
      JSON.stringify([{ name: "test", directory: "/tmp/test" }]),
    );
    const baseInvoke = mocks.invoke.getMockImplementation()!;
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "list_project_sessions" && args?.workingDirectory === "/tmp/test") {
        return { hasMore: false, sessions: [testSession] };
      }
      if (command === "switch_pi_session") {
        return {
          switched: true,
          snapshot: {
            ...readySnapshot,
            workingDirectory: "/tmp/test",
            sessionFile: testSession.path,
            sessionId: "session-target",
          },
        };
      }
      return baseInvoke(command, args);
    });

    await renderConnected();
    fireEvent.click(screen.getByRole("button", { name: "test 历史会话" }));
    const project = await screen.findByRole("region", { name: "test历史会话列表" });
    await within(project).findByText("测试会话");

    fireEvent.click(within(project).getByRole("button", { name: /测试会话/ }));
    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("switch_pi_session", {
        sessionPath: "/tmp/test/sessions/target.jsonl",
      }),
    );
    expect(await screen.findByText("历史提问")).toBeInTheDocument();

    // 模拟事件通道迟到投递：connect_pi 在目标目录建连时的 SessionReady，
    // 其 sessionId 是 Pi 新建的临时会话，与最终目标会话不同。
    emit({
      sequence: 90,
      event: {
        type: "session_ready",
        snapshot: {
          ...readySnapshot,
          workingDirectory: "/tmp/test",
          sessionFile: "/tmp/test/sessions/temp.jsonl",
          sessionId: "session-temp",
        },
      },
    });

    // 迟到快照不得把“当前会话”覆盖回临时会话：项目分组不弹兑底行，
    // 已加载的对话也不被清空。（重新查询：React 会重建列表节点，缓存引用会读到脱档的旧 DOM。）
    const projectAfterLate = screen.getByRole("region", { name: "test历史会话列表" });
    expect(
      within(projectAfterLate).queryByRole("button", { name: /^test$/ }),
    ).not.toBeInTheDocument();
    expect(within(projectAfterLate).getByRole("button", { name: /测试会话/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByText("历史提问")).toBeInTheDocument();
  });

  it("跨项目打开会话期间保留最近列表，并静默完成切换", async () => {
    localStorage.setItem(
      "lure:projects",
      JSON.stringify([{ name: "lure-project", directory: "/tmp/lure-project" }]),
    );
    await renderConnected();

    fireEvent.click(screen.getByRole("button", { name: "lure-project 历史会话" }));
    await screen.findByText("项目里的对话");

    const baseInvoke = mocks.invoke.getMockImplementation()!;
    const pendingSwitch = deferred<unknown>();
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "switch_pi_session") return pendingSwitch.promise;
      return baseInvoke(command, args);
    });

    fireEvent.click(screen.getByRole("button", { name: /项目里的对话/ }));

    await waitFor(() =>
      expect(mocks.invoke).toHaveBeenCalledWith("connect_pi", {
        workingDirectory: "/tmp/lure-project",
      }),
    );
    const recent = screen.getByRole("region", { name: "最近" });
    // 重建 Pi RPC 与加载目标会话期间，“最近”不清空、不重新扫描成空列表。
    expect(within(recent).getByText("昨天的工作")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /项目里的对话/ })).toHaveAttribute(
        "aria-busy",
        "true",
      ),
    );
    // 切换事务进行中：连接指向新目录的临时会话，但项目分组不得以此弹出“当前任务”兔底行。
    const projectDuringSwitch = screen.getByRole("region", { name: "lure-project历史会话列表" });
    expect(
      within(projectDuringSwitch).queryByRole("button", { name: /^lure-project$/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /项目里的对话/ })).toHaveAttribute("aria-busy", "true");
    // 切换过程不显示连接或加载提示。
    expect(screen.queryByText(/正在打开|正在连接|正在启动/)).not.toBeInTheDocument();
    // 整个切换事务期间不接受新的切换请求。
    expect(within(recent).getByRole("button", { name: /昨天的工作/ })).toBeDisabled();

    pendingSwitch.resolve({
      switched: true,
      snapshot: {
        ...readySnapshot,
        workingDirectory: "/tmp/lure-project",
        sessionFile: "/tmp/lure-project/sessions/new.jsonl",
        sessionId: "session-project",
      },
    });

    expect(await screen.findByText("历史提问")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: /项目里的对话/ })).toHaveAttribute(
        "aria-busy",
        "false",
      ),
    );
  });
});

describe("响应组与执行过程", () => {
  it("多轮工具调用后只有最后一条纯文本消息成为最终结果", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "重构模块" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "9", message: "重构模块" },
    });
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: {
        type: "assistant_message_completed",
        text: "第一步：读取入口",
        thinking: "",
        stopReason: "toolUse",
      },
    });
    emit({
      sequence: 5,
      event: {
        type: "tool_started",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\": \"src/App.tsx\"}",
      },
    });
    emit({
      sequence: 6,
      event: {
        type: "tool_completed",
        toolCallId: "tool-1",
        toolName: "read",
        input: "{\"path\": \"src/App.tsx\"}",
        output: "export default App",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({ sequence: 7, event: { type: "assistant_message_started" } });
    emit({
      sequence: 8,
      event: {
        type: "assistant_message_completed",
        text: "第二步：改写结构",
        thinking: "",
        stopReason: "toolUse",
      },
    });
    emit({
      sequence: 9,
      event: {
        type: "tool_started",
        toolCallId: "tool-2",
        toolName: "edit",
        input: "{\"path\": \"src/App.tsx\"}",
      },
    });
    emit({
      sequence: 10,
      event: {
        type: "tool_completed",
        toolCallId: "tool-2",
        toolName: "edit",
        input: "{\"path\": \"src/App.tsx\"}",
        output: "updated src/App.tsx",
        truncatedLines: null,
        isError: false,
      },
    });
    emit({ sequence: 11, event: { type: "assistant_message_started" } });
    emit({
      sequence: 12,
      event: {
        type: "assistant_message_completed",
        text: "重构已经完成",
        thinking: "",
        stopReason: "stop",
      },
    });
    emit({ sequence: 13, event: { type: "run_finished", willRetry: false } });
    emit({ sequence: 14, event: { type: "run_settled" } });

    // 一次用户指令只产生一个响应组，最终结果始终在折叠区之外。
    expect(screen.getAllByLabelText("Pi 回复")).toHaveLength(1);
    expect(await screen.findByText("重构已经完成")).toBeVisible();

    const toggle = screen.getByRole("button", { name: /2 个工具调用|用时/ });
    expect(toggle.getAttribute("aria-label")).toContain("展开执行过程");
    expect(screen.getByText("第一步：读取入口")).not.toBeVisible();

    fireEvent.click(toggle);
    expect(screen.getByText("第一步：读取入口")).toBeVisible();
    expect(screen.getByText("第二步：改写结构")).toBeVisible();
    expect(screen.getByRole("button", { name: /已读取 src\/App.tsx/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /已修改 src\/App.tsx/ })).toBeInTheDocument();
  });

  it("agent_end 之后 agent_settled 之前不提前定案", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "重试任务" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "10", message: "重试任务" },
    });
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: {
        type: "assistant_message_completed",
        text: "第一次响应",
        thinking: "",
        stopReason: "stop",
      },
    });
    emit({ sequence: 5, event: { type: "run_finished", willRetry: true } });

    // 自动重试仍可能继续，此时不出现代表定案的耗时标签。
    expect(screen.queryByRole("button", { name: /展开执行过程/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /用时/ })).not.toBeInTheDocument();

    emit({ sequence: 6, event: { type: "assistant_message_started" } });
    emit({
      sequence: 7,
      event: {
        type: "assistant_message_completed",
        text: "重试成功",
        thinking: "",
        stopReason: "stop",
      },
    });
    emit({ sequence: 8, event: { type: "run_finished", willRetry: false } });
    emit({ sequence: 9, event: { type: "turn_ended", stopReason: "stop" } });
    emit({ sequence: 10, event: { type: "run_settled" } });

    expect(await screen.findByText("重试成功")).toBeVisible();
    const toggle = screen.getByRole("button", { name: /展开执行过程/ });
    fireEvent.click(toggle);
    expect(screen.getByText("第一次响应")).toBeVisible();
  });

  it("被截断或被停止的响应只显示部分结果与说明", async () => {
    await renderConnected();
    fireEvent.change(textbox(), { target: { value: "长回答" } });
    fireEvent.submit(textbox().closest("form")!);
    await waitFor(() => expect(mocks.invoke).toHaveBeenCalledWith("send_prompt", expect.anything()));

    emit({
      sequence: 1,
      event: { type: "user_message_accepted", requestId: "11", message: "长回答" },
    });
    emit({ sequence: 2, event: { type: "run_started" } });
    emit({ sequence: 3, event: { type: "assistant_message_started" } });
    emit({
      sequence: 4,
      event: {
        type: "assistant_message_completed",
        text: "写了一半",
        thinking: "",
        stopReason: "length",
      },
    });
    emit({ sequence: 5, event: { type: "run_settled" } });

    expect(await screen.findByText("写了一半")).toBeInTheDocument();
    expect(
      screen.getByText("响应在完成前被截断，可以继续追问以补全结果。"),
    ).toBeInTheDocument();
    expect(screen.queryByText("任务执行完成")).not.toBeInTheDocument();
  });

  it("历史会话按同一响应组规则重建，过程默认折叠", async () => {
    const passthrough = mocks.invoke.getMockImplementation();
    mocks.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "get_session_entries") {
        return {
          entries: [
            {
              type: "message",
              id: "e1",
              parentId: null,
              message: { role: "user", content: "检查配置" },
            },
            {
              type: "message",
              id: "e2",
              parentId: "e1",
              message: {
                role: "assistant",
                stopReason: "toolUse",
                content: [
                  { type: "text", text: "先读取配置" },
                  {
                    type: "toolCall",
                    id: "tool-1",
                    name: "read",
                    arguments: { path: "package.json" },
                  },
                ],
              },
            },
            {
              type: "message",
              id: "e3",
              parentId: "e2",
              message: {
                role: "toolResult",
                toolCallId: "tool-1",
                toolName: "read",
                isError: false,
                content: [{ type: "text", text: "{\"name\":\"lure\"}" }],
              },
            },
            {
              type: "message",
              id: "e4",
              parentId: "e3",
              message: {
                role: "assistant",
                stopReason: "stop",
                content: [{ type: "text", text: "配置已确认" }],
              },
            },
          ],
          leafId: "e4",
        };
      }
      return passthrough?.(command, args);
    });

    await renderConnected();
    const recent = screen.getByRole("region", { name: "最近" });
    fireEvent.click(within(recent).getByRole("button", { name: /昨天的工作/ }));

    // 历史没有可靠的运行耗时，只用过程内容说明，不伪造用时。
    expect(await screen.findByText("配置已确认")).toBeVisible();
    const toggle = screen.getByRole("button", { name: "1 个工具调用 · 展开执行过程" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("先读取配置")).not.toBeVisible();

    fireEvent.click(toggle);
    expect(screen.getByText("先读取配置")).toBeVisible();
    expect(screen.getByRole("button", { name: /已读取 package.json/ })).toBeInTheDocument();
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
    expect(screen.queryByText("输入")).not.toBeInTheDocument();

    fireEvent.click(summary);
    expect(summary).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("输入")).toBeInTheDocument();
    expect(screen.getByText("输出")).toBeInTheDocument();
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

  it("连接中断时弹窗说明影响与原因，并提供可执行操作", async () => {
    await renderConnected();

    emit({ sequence: 1, event: { type: "process_exited", code: 1 } });

    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("影响：");
    expect(dialog.textContent).toContain("原因：");
    expect(within(dialog).getByRole("button", { name: "重新连接" })).toBeInTheDocument();
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
