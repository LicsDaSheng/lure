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

  it("加载项目目录，并按文件夹去重更新项目名称", () => {
    const store = configureStore({ reducer: { piConnection: piConnectionReducer } });

    store.dispatch(piConnectionActions.projectCatalogLoaded({
      defaultWorkspace: "/home/test/lure",
      projects: [],
    }));
    store.dispatch(piConnectionActions.projectAdded({
      directory: "/home/test/lure",
      name: "不应加入的默认目录",
    }));
    store.dispatch(piConnectionActions.projectAdded({
      directory: "/tmp/project",
      name: "project",
    }));
    store.dispatch(piConnectionActions.projectAdded({
      directory: "/tmp/project",
      name: "自定义名称",
    }));

    expect(store.getState().piConnection).toMatchObject({
      defaultWorkspace: "/home/test/lure",
      projects: [
        { directory: "/tmp/project", name: "自定义名称" },
      ],
    });
  });

  it("保存 Pi 记录的会话列表，并从会话条目重建历史对话", () => {
    const store = configureStore({ reducer: { piConnection: piConnectionReducer } });
    const sessions = [
      {
        path: "/home/me/.pi/agent/sessions/--tmp--/one.jsonl",
        id: "one",
        cwd: "/tmp/lure",
        name: null,
        parentSessionPath: null,
        createdAtMs: 1,
        modifiedAtMs: 2,
        messageCount: 2,
        firstMessage: "历史提问",
      },
    ];

    store.dispatch(
      piConnectionActions.projectCatalogLoaded({ defaultWorkspace: "/tmp/lure", projects: [] }),
    );
    store.dispatch(piConnectionActions.sessionsLoaded({
      append: false,
      directory: "/tmp/lure",
      hasMore: false,
      sessions,
    }));
    store.dispatch(
      piConnectionActions.historyLoaded({
        entries: [
          {
            type: "message",
            id: "e1",
            parentId: null,
            message: { role: "user", content: "历史提问" },
          },
          {
            type: "message",
            id: "e2",
            parentId: "e1",
            message: { role: "assistant", content: [{ type: "text", text: "历史回复" }] },
          },
        ],
        leafId: "e2",
      }),
    );

    expect(store.getState().piConnection.recentSessions).toEqual(sessions);
    expect(store.getState().piConnection.messages.map((message) => message.parts[0])).toMatchObject([
      { type: "text", text: "历史提问" },
      { type: "text", text: "历史回复" },
    ]);
  });
});
