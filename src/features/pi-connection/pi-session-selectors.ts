import { createSelector } from "@reduxjs/toolkit";

import type { RootState } from "@/app/store";

export const selectPiConnection = (state: RootState) => state.piConnection;
export const selectPiSessionState = createSelector(selectPiConnection, (state) => state);
export const selectConnection = createSelector(selectPiConnection, (state) => state.connection);
export const selectMessages = createSelector(selectPiConnection, (state) => state.messages);
export const selectRecentConversations = createSelector(selectPiConnection, (state) =>
  [...state.recentConversations].sort((left, right) => right.updatedAt - left.updatedAt),
);
export const selectSessionError = createSelector(
  selectPiConnection,
  (state) => state.commandError ?? state.error,
);
export const selectCanSend = createSelector(selectConnection, (connection) => connection.phase === "ready");
export const selectIsRunning = createSelector(selectConnection, (connection) => connection.phase === "running");
