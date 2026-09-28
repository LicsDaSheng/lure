import { configureStore } from "@reduxjs/toolkit";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sessionsReducer } from "@/features/sessions/sessions-slice";

const mocks = vi.hoisted(() => ({
  abortPi: vi.fn(),
  clearPiQueue: vi.fn(),
  followUpPi: vi.fn(),
  sendPrompt: vi.fn(),
  steerPi: vi.fn(),
}));
vi.mock("@/lib/pi-rpc/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pi-rpc/client")>()),
  ...mocks,
}));

import { conversationListenerMiddleware } from "./conversation-listeners";
import { conversationActions, conversationReducer } from "./conversation-slice";

function createStore() {
  return configureStore({
    reducer: { conversation: conversationReducer, sessions: sessionsReducer },
    middleware: (defaults) =>
      defaults().prepend(conversationListenerMiddleware.middleware),
  });
}

beforeEach(() => {
  mocks.abortPi.mockReset();
  mocks.clearPiQueue.mockReset();
  mocks.followUpPi.mockReset();
  mocks.sendPrompt.mockReset();
  mocks.steerPi.mockReset();
});

describe("conversation listeners", () => {
  it("发送成功后记录提交", async () => {
    mocks.sendPrompt.mockResolvedValue({ accepted: true });
    const store = createStore();
    store.dispatch(
      conversationActions.promptRequested({ message: "hello", images: [] }),
    );
    await vi.waitFor(() =>
      expect(store.getState().conversation.promptSubmissionCount).toBe(1),
    );
  });

  it("发送失败时写入 sessions 错误", async () => {
    mocks.sendPrompt.mockRejectedValue(new Error("发送失败"));
    const store = createStore();
    store.dispatch(
      conversationActions.promptRequested({ message: "hello", images: [] }),
    );
    await vi.waitFor(() =>
      expect(store.getState().sessions.commandError?.message).toBe("发送失败"),
    );
  });

  it("转发停止操作及其失败", async () => {
    mocks.abortPi.mockRejectedValue(new Error("停止失败"));
    const store = createStore();
    store.dispatch(conversationActions.abortRequested());
    await vi.waitFor(() =>
      expect(store.getState().sessions.commandError?.message).toBe("停止失败"),
    );
    expect(mocks.abortPi).toHaveBeenCalledOnce();
  });

  it("插队引导转发 steer 命令", async () => {
    mocks.steerPi.mockResolvedValue({ accepted: true });
    const store = createStore();
    store.dispatch(
      conversationActions.steerRequested({ message: "先停下重构", images: [] }),
    );
    await vi.waitFor(() =>
      expect(mocks.steerPi).toHaveBeenCalledWith("先停下重构", []),
    );
  });

  it("插队引导失败时写入 sessions 错误", async () => {
    mocks.steerPi.mockRejectedValue(new Error("插队引导失败"));
    const store = createStore();
    store.dispatch(
      conversationActions.steerRequested({ message: "先停下重构", images: [] }),
    );
    await vi.waitFor(() =>
      expect(store.getState().sessions.commandError?.message).toBe(
        "插队引导失败",
      ),
    );
  });

  it("排队后续转发 follow_up 命令", async () => {
    mocks.followUpPi.mockResolvedValue({ accepted: true });
    const store = createStore();
    store.dispatch(
      conversationActions.followUpRequested({
        message: "接着补测试",
        images: [],
      }),
    );
    await vi.waitFor(() =>
      expect(mocks.followUpPi).toHaveBeenCalledWith("接着补测试", []),
    );
  });

  it("清空队列转发 clear_queue 命令及其失败", async () => {
    mocks.clearPiQueue.mockRejectedValue(new Error("清空队列失败"));
    const store = createStore();
    store.dispatch(conversationActions.queueClearRequested());
    await vi.waitFor(() =>
      expect(store.getState().sessions.commandError?.message).toBe(
        "清空队列失败",
      ),
    );
    expect(mocks.clearPiQueue).toHaveBeenCalledOnce();
  });
});
