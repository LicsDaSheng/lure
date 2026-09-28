import { configureStore } from "@reduxjs/toolkit";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sessionsReducer } from "@/features/sessions/sessions-slice";

const mocks = vi.hoisted(() => ({
  selectModel: vi.fn(),
  selectThinkingLevel: vi.fn(),
}));
vi.mock("@/lib/pi-rpc/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pi-rpc/client")>()),
  ...mocks,
}));

import { modelsListenerMiddleware } from "./models-listeners";
import { modelsActions, modelsReducer } from "./models-slice";

function createStore() {
  return configureStore({
    reducer: { models: modelsReducer, sessions: sessionsReducer },
    middleware: (defaults) =>
      defaults().prepend(modelsListenerMiddleware.middleware),
  });
}

beforeEach(() => {
  mocks.selectModel.mockReset();
  mocks.selectThinkingLevel.mockReset();
});

describe("models listeners", () => {
  it("保存 Pi 确认的模型与 thinking level", async () => {
    mocks.selectModel.mockResolvedValue({ provider: "test", id: "model-b" });
    mocks.selectThinkingLevel.mockResolvedValue("high");
    const store = createStore();
    store.dispatch(
      modelsActions.modelSelectionRequested({
        provider: "test",
        id: "model-b",
      }),
    );
    store.dispatch(modelsActions.thinkingLevelSelectionRequested("high"));
    await vi.waitFor(() =>
      expect(store.getState().models).toMatchObject({
        current: { provider: "test", id: "model-b" },
        thinkingLevel: "high",
      }),
    );
  });

  it("模型切换失败时写入 sessions 错误", async () => {
    mocks.selectModel.mockRejectedValue(new Error("模型失败"));
    const store = createStore();
    store.dispatch(
      modelsActions.modelSelectionRequested({ provider: "test", id: "bad" }),
    );
    await vi.waitFor(() =>
      expect(store.getState().sessions.commandError?.message).toBe("模型失败"),
    );
  });
});
