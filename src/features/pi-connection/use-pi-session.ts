import { useCallback } from "react";

import { useAppDispatch, useAppSelector } from "@/app/hooks";

import type { ImageAttachment } from "./api";
import { piConnectionActions } from "./pi-session-slice";
import {
  selectPiConnection,
  selectRecentConversations,
  selectSessionError,
} from "./pi-session-selectors";

/** 将组件意图映射为 Redux action；IPC 和 Pi 事件订阅只由 listener middleware 执行。 */
export function usePiSession() {
  const state = useAppSelector(selectPiConnection);
  const error = useAppSelector(selectSessionError);
  const recentConversations = useAppSelector(selectRecentConversations);
  const dispatch = useAppDispatch();

  return {
    state,
    selectedDirectory: state.selectedDirectory,
    eventsReady: state.eventsReady,
    error,
    availableModels: state.availableModels,
    commands: state.commands,
    workspaceContext: state.workspaceContext,
    defaultWorkspace: state.defaultWorkspace,
    projects: state.projects,
    recentConversations,
    connect: useCallback(() => dispatch(piConnectionActions.connectRequested()), [dispatch]),
    retry: useCallback(() => dispatch(piConnectionActions.retryRequested()), [dispatch]),
    newConversation: useCallback(() => dispatch(piConnectionActions.newConversationRequested()), [dispatch]),
    newDefaultConversation: useCallback(
      () => dispatch(piConnectionActions.defaultConversationRequested()),
      [dispatch],
    ),
    newProjectConversation: useCallback(
      (directory: string) => dispatch(piConnectionActions.projectConversationRequested(directory)),
      [dispatch],
    ),
    addProject: useCallback(
      (name: string, directory: string) =>
        dispatch(piConnectionActions.projectAdded({ directory, name })),
      [dispatch],
    ),
    renameRecentConversation: useCallback(
      (title: string) => dispatch(piConnectionActions.recentConversationTitleChanged(title)),
      [dispatch],
    ),
    disconnect: useCallback(() => dispatch(piConnectionActions.disconnectRequested()), [dispatch]),
    prompt: useCallback(
      (message: string, images: ImageAttachment[] = []) =>
        dispatch(piConnectionActions.promptRequested({ images, message })),
      [dispatch],
    ),
    abort: useCallback(() => dispatch(piConnectionActions.abortRequested()), [dispatch]),
    setModel: useCallback(
      (provider: string, id: string) => dispatch(piConnectionActions.modelSelectionRequested({ provider, id })),
      [dispatch],
    ),
    setThinkingLevel: useCallback(
      (level: string) => dispatch(piConnectionActions.thinkingLevelSelectionRequested(level)),
      [dispatch],
    ),
    respondToExtension: useCallback(
      (value: unknown, cancelled = false) =>
        dispatch(piConnectionActions.extensionResponseRequested({ cancelled, value })),
      [dispatch],
    ),
  };
}
