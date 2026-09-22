import { configureStore } from "@reduxjs/toolkit";
import { describe, expect, it } from "vitest";

import {
  selectExpandedProjects,
  selectLoadingDirectories,
  selectProjectSessions,
  selectRecentSessions,
} from "./pi-session-selectors";
import { piConnectionActions, piConnectionReducer } from "./pi-session-slice";
import type { PiSessionSummary } from "./pi-session-types";

function summary(id: string, modifiedAtMs: number, cwd = "/tmp/lure"): PiSessionSummary {
  return {
    path: `/home/me/.pi/agent/sessions/--tmp--/${id}.jsonl`,
    id,
    cwd,
    name: null,
    parentSessionPath: null,
    createdAtMs: modifiedAtMs,
    modifiedAtMs,
    messageCount: 1,
    firstMessage: null,
  };
}

function storeWithCatalog() {
  const store = configureStore({ reducer: { piConnection: piConnectionReducer } });
  store.dispatch(
    piConnectionActions.projectCatalogLoaded({
      defaultWorkspace: "/tmp/lure",
      projects: [],
    }),
  );
  return store;
}

describe("Pi connection selectors", () => {
  it("默认工作目录的会话进入最近，并按最近活动时间倒序派生", () => {
    const store = storeWithCatalog();
    const stored = [summary("older", 10), summary("latest", 20)];
    store.dispatch(
      piConnectionActions.sessionsLoaded({ append: false, directory: "/tmp/lure", hasMore: false, sessions: stored }),
    );

    expect(selectRecentSessions(store.getState()).map((item) => item.id)).toEqual([
      "latest",
      "older",
    ]);
    expect(store.getState().piConnection.recentSessions).toEqual(stored);
  });

  it("其他目录的会话保存在对应项目下，不进入最近", () => {
    const store = storeWithCatalog();
    store.dispatch(
      piConnectionActions.sessionsLoaded({
        append: false,
        directory: "/tmp/project",
        hasMore: false,
        sessions: [summary("project-one", 30, "/tmp/project")],
      }),
    );

    expect(selectRecentSessions(store.getState())).toEqual([]);
    expect(selectProjectSessions(store.getState())["/tmp/project"]?.map((item) => item.id)).toEqual([
      "project-one",
    ]);
  });

  it("记录项目展开状态与正在读取的目录", () => {
    const store = storeWithCatalog();

    store.dispatch(piConnectionActions.projectExpansionToggled("/tmp/project"));
    store.dispatch(piConnectionActions.sessionsRequested("/tmp/project"));
    expect(selectExpandedProjects(store.getState())).toEqual(["/tmp/project"]);
    expect(selectLoadingDirectories(store.getState())).toEqual(["/tmp/project"]);

    store.dispatch(
      piConnectionActions.sessionsLoaded({ append: false, directory: "/tmp/project", hasMore: false, sessions: [] }),
    );
    expect(selectLoadingDirectories(store.getState())).toEqual([]);

    store.dispatch(piConnectionActions.projectExpansionToggled("/tmp/project"));
    expect(selectExpandedProjects(store.getState())).toEqual([]);
  });
});
