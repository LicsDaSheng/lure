import { createSelector } from "@reduxjs/toolkit";

import type { RootState } from "@/app/store";

import type { PiSessionSummary } from "./pi-session-types";

export const selectPiConnection = (state: RootState) => state.piConnection;
export const selectPiSessionState = createSelector(selectPiConnection, (state) => state);
export const selectConnection = createSelector(selectPiConnection, (state) => state.connection);
export const selectMessages = createSelector(selectPiConnection, (state) => state.messages);

/** 会话列表按最近活动时间倒序展示；store 只保存 Pi 侧的事实。 */
function byRecentActivity(sessions: PiSessionSummary[]): PiSessionSummary[] {
  return [...sessions].sort((left, right) => right.modifiedAtMs - left.modifiedAtMs);
}

export const selectRecentSessions = createSelector(selectPiConnection, (state) =>
  byRecentActivity(state.recentSessions),
);
export const selectProjectSessions = createSelector(selectPiConnection, (state) =>
  Object.fromEntries(
    Object.entries(state.projectSessions).map(([directory, sessions]) => [
      directory,
      byRecentActivity(sessions),
    ]),
  ),
);
export const selectExpandedProjects = createSelector(
  selectPiConnection,
  (state) => state.expandedProjects,
);
export const selectLoadingDirectories = createSelector(
  selectPiConnection,
  (state) => state.loadingDirectories,
);
export const selectSessionError = createSelector(
  selectPiConnection,
  (state) => state.commandError ?? state.error,
);
export const selectCanSend = createSelector(
  selectConnection,
  (connection) => connection.phase === "ready",
);
export const selectIsRunning = createSelector(
  selectConnection,
  (connection) => connection.phase === "running",
);