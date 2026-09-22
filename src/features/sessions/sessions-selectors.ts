import { createSelector } from "@reduxjs/toolkit";

import type { RootState } from "@/app/store";
import type { PiSessionSummary } from "@/lib/pi-rpc/types";

export const selectSessions = (state: RootState) => state.sessions;
export const selectSessionConnection = createSelector(selectSessions, (state) => state.connection);

function byRecentActivity(sessions: PiSessionSummary[]): PiSessionSummary[] {
  return [...sessions].sort((left, right) => right.modifiedAtMs - left.modifiedAtMs);
}

export const selectRecentSessions = createSelector(selectSessions, (state) =>
  byRecentActivity(state.recentSessions),
);
export const selectProjectSessions = createSelector(selectSessions, (state) =>
  Object.fromEntries(
    Object.entries(state.projectSessions).map(([directory, sessions]) => [
      directory,
      byRecentActivity(sessions),
    ]),
  ),
);
export const selectExpandedProjects = createSelector(selectSessions, (state) => state.expandedProjects);
export const selectLoadingDirectories = createSelector(selectSessions, (state) => state.loadingDirectories);
export const selectSessionError = createSelector(selectSessions, (state) => state.commandError ?? state.error);
export const selectCanSend = createSelector(selectSessionConnection, (connection) => connection.phase === "ready");
export const selectIsRunning = createSelector(selectSessionConnection, (connection) => connection.phase === "running");
