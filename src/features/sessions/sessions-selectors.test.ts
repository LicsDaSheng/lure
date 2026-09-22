import { configureStore } from "@reduxjs/toolkit";
import { describe, expect, it } from "vitest";

import type { RootState } from "@/app/store";
import type { PiSessionSummary } from "@/lib/pi-rpc/types";
import { sessionsActions, sessionsReducer } from "./sessions-slice";
import {
  selectExpandedProjects,
  selectLoadingDirectories,
  selectProjectSessions,
  selectRecentSessions,
} from "./sessions-selectors";

function summary(id: string, modifiedAtMs: number, cwd = "/tmp/lure"): PiSessionSummary {
  return {
    path: `/home/me/.pi/agent/sessions/--tmp--/${id}.jsonl`, id, cwd,
    name: null, parentSessionPath: null, createdAtMs: modifiedAtMs,
    modifiedAtMs, messageCount: 1, firstMessage: null,
  };
}

function storeWithCatalog() {
  const store = configureStore({ reducer: { sessions: sessionsReducer } });
  store.dispatch(sessionsActions.projectCatalogLoaded({ defaultWorkspace: "/tmp/lure", projects: [] }));
  return store;
}

function rootState(store: ReturnType<typeof storeWithCatalog>): RootState {
  return store.getState() as RootState;
}

describe("sessions selectors", () => {
  it("按最近活动时间派生默认工作目录的会话", () => {
    const store = storeWithCatalog();
    const stored = [summary("older", 10), summary("latest", 20)];
    store.dispatch(sessionsActions.sessionsLoaded({ append: false, directory: "/tmp/lure", hasMore: false, sessions: stored }));
    expect(selectRecentSessions(rootState(store)).map((item) => item.id)).toEqual(["latest", "older"]);
    expect(store.getState().sessions.recentSessions).toEqual(stored);
  });

  it("按项目目录隔离会话", () => {
    const store = storeWithCatalog();
    store.dispatch(sessionsActions.sessionsLoaded({ append: false, directory: "/tmp/project", hasMore: false, sessions: [summary("project-one", 30, "/tmp/project")] }));
    expect(selectRecentSessions(rootState(store))).toEqual([]);
    expect(selectProjectSessions(rootState(store))["/tmp/project"]?.map((item) => item.id)).toEqual(["project-one"]);
  });

  it("记录项目展开和加载状态", () => {
    const store = storeWithCatalog();
    store.dispatch(sessionsActions.projectExpansionToggled("/tmp/project"));
    store.dispatch(sessionsActions.sessionsRequested("/tmp/project"));
    expect(selectExpandedProjects(rootState(store))).toEqual(["/tmp/project"]);
    expect(selectLoadingDirectories(rootState(store))).toEqual(["/tmp/project"]);
    store.dispatch(sessionsActions.sessionsLoaded({ append: false, directory: "/tmp/project", hasMore: false, sessions: [] }));
    expect(selectLoadingDirectories(rootState(store))).toEqual([]);
  });

  it("当前打开的会话不在当前页时仍保留在项目列表中", () => {
    const store = storeWithCatalog();
    store.dispatch(sessionsActions.sessionsLoaded({
      append: false,
      directory: "/tmp/project",
      hasMore: true,
      sessions: [summary("newer", 30, "/tmp/project"), summary("newest", 40, "/tmp/project")],
    }));
    store.dispatch(sessionsActions.activeSessionRecorded(summary("active", 5, "/tmp/project")));

    expect(selectProjectSessions(rootState(store))["/tmp/project"]?.map((item) => item.id))
      .toEqual(["newest", "newer", "active"]);
  });

  it("当前会话已在列表中时不重复出现", () => {
    const store = storeWithCatalog();
    const active = summary("active", 30, "/tmp/project");
    store.dispatch(sessionsActions.sessionsLoaded({
      append: false,
      directory: "/tmp/project",
      hasMore: false,
      sessions: [active],
    }));
    store.dispatch(sessionsActions.activeSessionRecorded(active));

    expect(selectProjectSessions(rootState(store))["/tmp/project"]).toHaveLength(1);
  });

  it("属于默认工作目录的当前会话保留在最近列表中", () => {
    const store = storeWithCatalog();
    store.dispatch(sessionsActions.sessionsLoaded({
      append: false,
      directory: "/tmp/lure",
      hasMore: true,
      sessions: [summary("newer", 30)],
    }));
    store.dispatch(sessionsActions.activeSessionRecorded(summary("active", 5)));

    expect(selectRecentSessions(rootState(store)).map((item) => item.id)).toEqual(["newer", "active"]);
  });
});
