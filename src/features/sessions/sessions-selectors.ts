import { createSelector } from "@reduxjs/toolkit";

import type { RootState } from "@/app/store";
import type { PiSessionSummary } from "@/lib/pi-rpc/types";

import type { SessionsState } from "./sessions-slice";

export const selectSessions = (state: RootState) => state.sessions;
export const selectSessionConnection = createSelector(selectSessions, (state) => state.connection);

function byRecentActivity(sessions: PiSessionSummary[]): PiSessionSummary[] {
  return [...sessions].sort((left, right) => right.modifiedAtMs - left.modifiedAtMs);
}

/**
 * 让当前打开的会话始终留在导航里。
 *
 * 分页只取最新几条，刷新后较旧的当前会话会掉出列表；此时导航不得退化成一条
 * 以项目名或本地标题命名的“当前任务”条目，而应继续展示真实会话本身。
 */
function withActiveSession(
  sessions: PiSessionSummary[],
  active: PiSessionSummary | null,
): PiSessionSummary[] {
  if (!active || sessions.some((session) => session.path === active.path)) return sessions;
  return [...sessions, active];
}

function activeSessionDirectory(state: SessionsState): string | null {
  const active = state.activeSessionSummary;
  if (!active) return null;
  return active.cwd ?? state.connection.workingDirectory;
}

export const selectRecentSessions = createSelector(selectSessions, (state) => {
  const directory = activeSessionDirectory(state);
  const inDefaultWorkspace = directory && directory === state.defaultWorkspace ? state.activeSessionSummary : null;
  return byRecentActivity(withActiveSession(state.recentSessions, inDefaultWorkspace));
});
export const selectProjectSessions = createSelector(selectSessions, (state) => {
  const active = state.activeSessionSummary;
  const directory = activeSessionDirectory(state);
  const entries: Record<string, PiSessionSummary[]> = { ...state.projectSessions };
  if (active && directory && directory !== state.defaultWorkspace) {
    entries[directory] = withActiveSession(entries[directory] ?? [], active);
  }
  return Object.fromEntries(
    Object.entries(entries).map(([key, sessions]) => [key, byRecentActivity(sessions)]),
  );
});
export const selectExpandedProjects = createSelector(selectSessions, (state) => state.expandedProjects);
export const selectSessionTransition = createSelector(selectSessions, (state) => state.sessionTransition);
export const selectLoadingDirectories = createSelector(selectSessions, (state) => state.loadingDirectories);
export const selectSessionError = createSelector(selectSessions, (state) => state.commandError);
export const selectConnectionError = createSelector(selectSessions, (state) => state.error);
export const selectCanSend = createSelector(selectSessionConnection, (connection) => connection.phase === "ready");
export const selectIsRunning = createSelector(selectSessionConnection, (connection) => connection.phase === "running");
