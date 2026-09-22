import { configureStore } from "@reduxjs/toolkit";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { conversationReducer } from "@/features/conversation/conversation-slice";
import { piSessionReducer } from "@/features/conversation/conversation-domain";
import { executionReducer } from "@/features/execution/execution-slice";
import { extensionUiReducer } from "@/features/extension-ui/extension-ui-slice";
import { modelsReducer } from "@/features/models/models-slice";
import type { EventEnvelope, PiSessionState } from "@/lib/pi-rpc/types";

const mocks = vi.hoisted(() => ({
  connectPi: vi.fn(),
  disconnectPi: vi.fn(),
  getAvailableModels: vi.fn(),
  getDefaultWorkspace: vi.fn(),
  getPiCommands: vi.fn(),
  getPiState: vi.fn(),
  getSessionEntries: vi.fn(),
  getWorkspaceContext: vi.fn(),
  listProjectSessions: vi.fn(),
  listenToPiEvents: vi.fn(),
  selectProjectDirectory: vi.fn(),
  switchPiSession: vi.fn(),
}));

vi.mock("@/lib/pi-rpc/client", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/pi-rpc/client")>();
  return { ...actual, ...mocks };
});

import { piRuntimeProjected } from "./runtime-events";
import { sessionsListenerMiddleware } from "./sessions-listeners";
import { sessionsActions, sessionsReducer } from "./sessions-slice";

let activeStore: ReturnType<typeof configureListenerStore> | null = null;

function configureListenerStore() {
  return configureStore({
    reducer: {
      sessions: sessionsReducer,
      conversation: conversationReducer,
      execution: executionReducer,
      models: modelsReducer,
      extensionUi: extensionUiReducer,
    },
    middleware: (getDefaultMiddleware) =>
      getDefaultMiddleware().prepend(sessionsListenerMiddleware.middleware),
  });
}

function combined(store: ReturnType<typeof configureListenerStore>) {
  const state = store.getState();
  return {
    ...state.sessions,
    ...state.conversation,
    ...state.execution,
    availableModels: state.models.availableModels,
    extensionRequest: state.extensionUi.request,
    connection: {
      ...state.sessions.connection,
      model: state.models.current,
      thinkingLevel: state.models.thinkingLevel,
    },
  };
}

function runtime(store: ReturnType<typeof configureListenerStore>): PiSessionState {
  const state = combined(store);
  return {
    connection: state.connection,
    messages: state.messages,
    activeAssistantId: state.activeAssistantId,
    error: state.error,
    notice: state.notice,
    diagnostics: state.diagnostics,
    run: state.run,
    extensionRequest: state.extensionRequest,
  };
}

function createStore() {
  activeStore = configureListenerStore();
  return activeStore;
}

async function flushListeners() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

beforeEach(() => {
  mocks.connectPi.mockReset();
  mocks.disconnectPi.mockReset();
  mocks.getAvailableModels.mockReset();
  mocks.getDefaultWorkspace.mockReset();
  mocks.getPiCommands.mockReset();
  mocks.getPiState.mockReset();
  mocks.getSessionEntries.mockReset();
  mocks.getWorkspaceContext.mockReset();
  mocks.listProjectSessions.mockReset();
  mocks.listenToPiEvents.mockReset();
  mocks.selectProjectDirectory.mockReset();
  mocks.switchPiSession.mockReset();
  mocks.getDefaultWorkspace.mockResolvedValue("/tmp/lure");
  mocks.getPiState.mockResolvedValue({
    error: null,
    model: null,
    phase: "disconnected",
    sessionFile: null,
    sessionId: null,
    thinkingLevel: null,
    workingDirectory: null,
  });
  mocks.connectPi.mockResolvedValue({
    error: null,
    model: null,
    phase: "ready",
    sessionFile: "/tmp/session.jsonl",
    sessionId: "session-1",
    thinkingLevel: "medium",
    workingDirectory: "/tmp/lure",
  });
  mocks.getAvailableModels.mockResolvedValue([{ id: "gpt-5", provider: "openai" }]);
  mocks.getPiCommands.mockResolvedValue([]);
  mocks.getWorkspaceContext.mockResolvedValue({ branch: "main", workingDirectory: "/tmp/lure" });
  mocks.listProjectSessions.mockResolvedValue({ hasMore: false, sessions: [] });
  mocks.selectProjectDirectory.mockResolvedValue(null);
});

afterEach(async () => {
  // 每个用例都释放订阅，避免 listener 的模块级订阅状态泄漏到下一例。
  activeStore?.dispatch(sessionsActions.eventSubscriptionReleased());
  activeStore = null;
  await flushListeners();
});

describe("Pi session listener middleware", () => {
  it("通过 listener 选择项目目录并写入 sessions 状态", async () => {
    mocks.selectProjectDirectory.mockResolvedValue("/tmp/project");
    const store = createStore();

    store.dispatch(sessionsActions.projectDirectorySelectionRequested());
    await flushListeners();

    expect(mocks.selectProjectDirectory).toHaveBeenCalledOnce();
    expect(store.getState().sessions.projectDirectoryCandidate).toBe("/tmp/project");
  });

  it("项目目录选择失败时写入归一化错误", async () => {
    mocks.selectProjectDirectory.mockRejectedValue(new Error("选择失败"));
    const store = createStore();

    store.dispatch(sessionsActions.projectDirectorySelectionRequested());
    await flushListeners();

    expect(store.getState().sessions.commandError).toEqual({
      code: "CLIENT_ERROR",
      message: "选择失败",
    });
  });

  it("订阅成功后启动默认工作区，并将 RPC 结果写回 store", async () => {
    const unlisten = vi.fn();
    mocks.listenToPiEvents.mockResolvedValue(unlisten);
    const store = createStore();

    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    expect(mocks.listenToPiEvents).toHaveBeenCalledOnce();
    expect(mocks.connectPi).toHaveBeenCalledWith("/tmp/lure");
    expect(combined(store)).toMatchObject({
      availableModels: [{ id: "gpt-5", provider: "openai" }],
      connection: { phase: "ready", sessionId: "session-1" },
      eventsReady: true,
      selectedDirectory: "/tmp/lure",
      workspaceContext: { branch: "main" },
    });
  });

  it("桌面端已有 Pi 会话时恢复快照而不重复连接", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.getPiState.mockResolvedValue({
      error: null,
      model: { id: "gpt-5", provider: "openai" },
      phase: "ready",
      sessionFile: "/tmp/session.jsonl",
      sessionId: "existing-session",
      thinkingLevel: "medium",
      workingDirectory: "/tmp/lure",
    });
    const store = createStore();

    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    expect(mocks.connectPi).not.toHaveBeenCalled();
    expect(combined(store).connection).toMatchObject({
      phase: "ready",
      sessionId: "existing-session",
    });
  });

  it("订阅失败时将归一化错误写回 store", async () => {
    mocks.listenToPiEvents.mockRejectedValue(new Error("监听失败"));
    const store = createStore();

    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    expect(combined(store).commandError).toEqual({
      code: "CLIENT_ERROR",
      message: "监听失败",
    });
  });

  it("释放订阅时调用清理函数并重置监听就绪状态", async () => {
    const unlisten = vi.fn();
    mocks.listenToPiEvents.mockResolvedValue(unlisten);
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    store.dispatch(sessionsActions.eventSubscriptionReleased());
    await flushListeners();

    expect(unlisten).toHaveBeenCalledOnce();
    expect(combined(store).eventsReady).toBe(false);
  });

  it("合并高频文本增量并以单个 Redux 批次更新", async () => {
    let onEvent: ((event: EventEnvelope) => void) | undefined;
    mocks.listenToPiEvents.mockImplementation(async (listener) => {
      onEvent = listener;
      return vi.fn();
    });
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    onEvent?.({ sequence: 1, event: { type: "assistant_message_started" } });
    let updates = 0;
    const unsubscribeStore = store.subscribe(() => { updates += 1; });
    for (let index = 2; index <= 1001; index += 1) {
      onEvent?.({
        sequence: index,
        event: { type: "assistant_text_delta", contentIndex: 0, delta: "x" },
      });
    }

    await new Promise((resolve) => setTimeout(resolve, 40));
    unsubscribeStore();

    const message = combined(store).messages[0];
    expect(message?.parts[0]).toMatchObject({ type: "text", text: "x".repeat(1000) });
    expect(updates).toBe(1);
  });
});

describe("历史会话", () => {
  const recordedSession = {
    path: "/tmp/lure/sessions/old.jsonl",
    id: "session-old",
    cwd: "/tmp/lure",
    name: null,
    parentSessionPath: null,
    createdAtMs: 1_700_000_000_000,
    modifiedAtMs: 1_700_000_000_000,
    messageCount: 2,
    firstMessage: "昨天的工作",
  };

  it("连接后只加载默认工作目录的历史会话到最近", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.listProjectSessions.mockResolvedValue({ hasMore: false, sessions: [recordedSession] });
    const store = createStore();

    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    expect(mocks.listProjectSessions).toHaveBeenCalledWith("/tmp/lure", 0, 3);
    expect(combined(store).recentSessions).toEqual([recordedSession]);
    expect(combined(store).projectSessions).toEqual({});
    expect(combined(store).loadingDirectories).toEqual([]);
  });

  it("展开项目时查询该项目的历史会话，折叠后不再查询", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.listProjectSessions.mockImplementation(async (directory: string) => ({
      hasMore: false,
      sessions: directory === "/tmp/project" ? [projectSession] : [recordedSession],
    }),
    );
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    store.dispatch(sessionsActions.projectExpansionToggled("/tmp/project"));
    await flushListeners();

    expect(mocks.listProjectSessions).toHaveBeenCalledWith("/tmp/project", 0, 3);
    expect(combined(store).projectSessions["/tmp/project"]).toEqual([
      projectSession,
    ]);
    // 展开项目不会改变“最近”的内容。
    expect(combined(store).recentSessions).toEqual([recordedSession]);

    mocks.listProjectSessions.mockClear();
    store.dispatch(sessionsActions.projectExpansionToggled("/tmp/project"));
    await flushListeners();

    expect(combined(store).expandedProjects).toEqual([]);
    expect(mocks.listProjectSessions).not.toHaveBeenCalled();
  });

  it("更多消息从后端读取下一页并追加到项目列表", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.listProjectSessions.mockResolvedValue({ hasMore: false, sessions: [] });
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();
    store.dispatch(sessionsActions.projectExpansionToggled("/tmp/project"));
    await flushListeners();

    store.dispatch(sessionsActions.sessionsLoaded({
      append: false,
      directory: "/tmp/project",
      hasMore: true,
      sessions: [projectSession],
    }));
    mocks.listProjectSessions.mockResolvedValue({
      hasMore: false,
      sessions: [{ ...projectSession, id: "session-older", path: "/tmp/project/older.jsonl" }],
    });
    store.dispatch(sessionsActions.sessionPageRequested("/tmp/project"));
    await flushListeners();

    expect(mocks.listProjectSessions).toHaveBeenLastCalledWith("/tmp/project", 1, 5);
    expect(combined(store).projectSessions["/tmp/project"]).toHaveLength(2);
    expect(combined(store).projectSessionsHasMore["/tmp/project"]).toBe(false);
  });

  it("打开其他项目的会话时先在该目录重建 RPC，再加载该会话", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.switchPiSession.mockResolvedValue({
      switched: true,
      snapshot: {
        error: null,
        model: null,
        phase: "ready",
        sessionFile: projectSession.path,
        sessionId: "session-project",
        thinkingLevel: "medium",
        workingDirectory: "/tmp/project",
      },
    });
    mocks.getSessionEntries.mockResolvedValue({ entries: [], leafId: null });
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    store.dispatch(sessionsActions.conversationOpenRequested(projectSession));
    await flushListeners();

    expect(mocks.disconnectPi).toHaveBeenCalled();
    expect(mocks.connectPi).toHaveBeenLastCalledWith("/tmp/project");
    expect(mocks.switchPiSession).toHaveBeenCalledWith(projectSession.path);
    expect(combined(store).connection.sessionId).toBe("session-project");
  });

  const projectSession = {
    path: "/tmp/project/sessions/new.jsonl",
    id: "session-project",
    cwd: "/tmp/project",
    name: null,
    parentSessionPath: null,
    createdAtMs: 1_700_000_000_000,
    modifiedAtMs: 1_700_000_000_000,
    messageCount: 1,
    firstMessage: "在项目里的对话",
  };

  it("打开历史会话时切换 Pi 会话并用条目重建对话", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.switchPiSession.mockResolvedValue({
      switched: true,
      snapshot: {
        error: null,
        model: null,
        phase: "ready",
        sessionFile: recordedSession.path,
        sessionId: "session-old",
        thinkingLevel: "medium",
        workingDirectory: "/tmp/lure",
      },
    });
    mocks.getSessionEntries.mockResolvedValue({
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
    });
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();

    store.dispatch(sessionsActions.conversationOpenRequested(recordedSession));
    await flushListeners();

    expect(mocks.switchPiSession).toHaveBeenCalledWith(recordedSession.path);
    expect(combined(store).connection.sessionId).toBe("session-old");
    expect(
      combined(store).messages.map((message) => message.parts[0]),
    ).toMatchObject([
      { type: "text", text: "历史提问" },
      { type: "text", text: "历史回复" },
    ]);
  });

  it("切换被 Pi 扩展取消时保留当前对话并说明原因", async () => {
    mocks.listenToPiEvents.mockResolvedValue(vi.fn());
    mocks.switchPiSession.mockResolvedValue({
      switched: false,
      snapshot: {
        error: null,
        model: null,
        phase: "ready",
        sessionFile: "/tmp/session.jsonl",
        sessionId: "session-1",
        thinkingLevel: "medium",
        workingDirectory: "/tmp/lure",
      },
    });
    const store = createStore();
    store.dispatch(sessionsActions.eventSubscriptionRequested());
    await flushListeners();
    const envelope: EventEnvelope = {
      sequence: 0,
      event: { type: "user_message_accepted", requestId: "keep", message: "当前对话" },
    };
    store.dispatch(piRuntimeProjected({
      envelopes: [envelope],
      state: piSessionReducer(runtime(store), envelope),
    }));

    store.dispatch(sessionsActions.conversationOpenRequested(recordedSession));
    await flushListeners();

    expect(mocks.getSessionEntries).not.toHaveBeenCalled();
    expect(combined(store).connection.sessionId).toBe("session-1");
    expect(combined(store).messages).toHaveLength(1);
    expect(combined(store).commandError).toEqual({
      code: "SESSION_SWITCH_CANCELLED",
      message: "Pi 扩展取消了这次会话切换，当前对话保持不变。",
    });
  });
});
