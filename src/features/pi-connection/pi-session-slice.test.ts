import { configureStore } from "@reduxjs/toolkit";
import { describe, expect, it } from "vitest";

import { piConnectionActions, piConnectionReducer } from "./pi-session-slice";

describe("Pi connection Redux slice", () => {
  it("将 Pi 事件归并为可观察的全局会话状态", () => {
    const store = configureStore({ reducer: { piConnection: piConnectionReducer } });

    store.dispatch(
      piConnectionActions.piEventReceived({
        sequence: 1,
        event: { type: "user_message_accepted", requestId: "request-1", message: "检查项目" },
      }),
    );

    expect(store.getState().piConnection.messages).toMatchObject([
      { id: "user-request-1", role: "user" },
    ]);
  });

  it("将本地能力与工作区上下文以 action 写回 store", () => {
    const store = configureStore({ reducer: { piConnection: piConnectionReducer } });

    store.dispatch(piConnectionActions.selectedDirectoryChanged("/tmp/lure"));
    store.dispatch(piConnectionActions.capabilitiesLoaded({
      commands: [{ description: "审查", name: "review", source: "extension" }],
      models: [{ id: "gpt-5", provider: "openai" }],
    }));
    store.dispatch(piConnectionActions.workspaceContextLoaded({
      branch: "main",
      workingDirectory: "/tmp/lure",
    }));

    store.dispatch(piConnectionActions.promptAccepted());

    expect(store.getState().piConnection).toMatchObject({
      availableModels: [{ id: "gpt-5", provider: "openai" }],
      promptSubmissionCount: 1,
      selectedDirectory: "/tmp/lure",
      workspaceContext: { branch: "main" },
    });
  });
});
