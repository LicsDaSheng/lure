import { configureStore } from "@reduxjs/toolkit";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { EventEnvelope } from "./pi-session-types";

const mocks = vi.hoisted(() => ({
  connectPi: vi.fn(),
  disconnectPi: vi.fn(),
  getAvailableModels: vi.fn(),
  getDefaultWorkspace: vi.fn(),
  getPiCommands: vi.fn(),
  getPiState: vi.fn(),
  getWorkspaceContext: vi.fn(),
  listenToPiEvents: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return { ...actual, ...mocks };
});

import { piSessionListenerMiddleware } from "./pi-session-listeners";
import { piConnectionActions, piConnectionReducer } from "./pi-session-slice";

let activeStore: ReturnType<typeof configureListenerStore> | null = null;

function configureListenerStore() {
  return configureStore({
    reducer: { piConnection: piConnectionReducer },
    middleware: (getDefaultMiddleware) =>
      getDefaultMiddleware().prepend(piSessionListenerMiddleware.middleware),
  });
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
  mocks.getWorkspaceContext.mockReset();
  mocks.listenToPiEvents.mockReset();
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
});

afterEach(async () => {
  // 每个用例都释放订阅，避免 listener 的模块级订阅状态泄漏到下一例。
  activeStore?.dispatch(piConnectionActions.eventSubscriptionReleased());
  activeStore = null;
  await flushListeners();
});

describe("Pi session listener middleware", () => {
  it("订阅成功后启动默认工作区，并将 RPC 结果写回 store", async () => {
    const unlisten = vi.fn();
    mocks.listenToPiEvents.mockResolvedValue(unlisten);
    const store = createStore();

    store.dispatch(piConnectionActions.eventSubscriptionRequested());
    await flushListeners();

    expect(mocks.listenToPiEvents).toHaveBeenCalledOnce();
    expect(mocks.connectPi).toHaveBeenCalledWith("/tmp/lure");
    expect(store.getState().piConnection).toMatchObject({
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

    store.dispatch(piConnectionActions.eventSubscriptionRequested());
    await flushListeners();

    expect(mocks.connectPi).not.toHaveBeenCalled();
    expect(store.getState().piConnection.connection).toMatchObject({
      phase: "ready",
      sessionId: "existing-session",
    });
  });

  it("订阅失败时将归一化错误写回 store", async () => {
    mocks.listenToPiEvents.mockRejectedValue(new Error("监听失败"));
    const store = createStore();

    store.dispatch(piConnectionActions.eventSubscriptionRequested());
    await flushListeners();

    expect(store.getState().piConnection.commandError).toEqual({
      code: "CLIENT_ERROR",
      message: "监听失败",
    });
  });

  it("释放订阅时调用清理函数并重置监听就绪状态", async () => {
    const unlisten = vi.fn();
    mocks.listenToPiEvents.mockResolvedValue(unlisten);
    const store = createStore();
    store.dispatch(piConnectionActions.eventSubscriptionRequested());
    await flushListeners();

    store.dispatch(piConnectionActions.eventSubscriptionReleased());
    await flushListeners();

    expect(unlisten).toHaveBeenCalledOnce();
    expect(store.getState().piConnection.eventsReady).toBe(false);
  });

  it("合并高频文本增量并以单个 Redux 批次更新", async () => {
    let onEvent: ((event: EventEnvelope) => void) | undefined;
    mocks.listenToPiEvents.mockImplementation(async (listener) => {
      onEvent = listener;
      return vi.fn();
    });
    const store = createStore();
    store.dispatch(piConnectionActions.eventSubscriptionRequested());
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

    const message = store.getState().piConnection.messages[0];
    expect(message?.parts[0]).toMatchObject({ type: "text", text: "x".repeat(1000) });
    expect(updates).toBe(1);
  });
});
