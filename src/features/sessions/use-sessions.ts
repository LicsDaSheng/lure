import { useCallback } from "react";

import { useAppDispatch, useAppSelector } from "@/app/hooks";
import type { PiSessionSummary } from "@/lib/pi-rpc/types";

import { sessionsActions } from "./sessions-slice";
import {
  selectConnectionError,
  selectExpandedProjects,
  selectLoadingDirectories,
  selectProjectSessions,
  selectRecentSessions,
  selectSessionError,
  selectSessions,
} from "./sessions-selectors";

export function useSessions() {
  const state = useAppSelector(selectSessions);
  const dispatch = useAppDispatch();
  return {
    ...state,
    error: useAppSelector(selectSessionError),
    connectionError: useAppSelector(selectConnectionError),
    recentSessions: useAppSelector(selectRecentSessions),
    projectSessions: useAppSelector(selectProjectSessions),
    expandedProjects: useAppSelector(selectExpandedProjects),
    loadingDirectories: useAppSelector(selectLoadingDirectories),
    retry: useCallback(
      () => dispatch(sessionsActions.retryRequested()),
      [dispatch],
    ),
    newDefaultConversation: useCallback(
      () => dispatch(sessionsActions.defaultConversationRequested()),
      [dispatch],
    ),
    newProjectConversation: useCallback(
      (directory: string) =>
        dispatch(sessionsActions.projectConversationRequested(directory)),
      [dispatch],
    ),
    addProject: useCallback(
      (name: string, directory: string) =>
        dispatch(sessionsActions.projectAdded({ directory, name })),
      [dispatch],
    ),
    toggleProject: useCallback(
      (directory: string) =>
        dispatch(sessionsActions.projectExpansionToggled(directory)),
      [dispatch],
    ),
    loadMoreSessions: useCallback(
      (directory: string) =>
        dispatch(sessionsActions.sessionPageRequested(directory)),
      [dispatch],
    ),
    openConversation: useCallback(
      (session: PiSessionSummary) =>
        dispatch(sessionsActions.conversationOpenRequested(session)),
      [dispatch],
    ),
    chooseProjectDirectory: useCallback(
      () => dispatch(sessionsActions.projectDirectorySelectionRequested()),
      [dispatch],
    ),
    clearProjectDirectory: useCallback(
      () => dispatch(sessionsActions.projectDirectorySelected(null)),
      [dispatch],
    ),
  };
}
