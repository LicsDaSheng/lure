import { createListenerMiddleware } from "@reduxjs/toolkit";
import type { Dispatch, UnknownAction } from "@reduxjs/toolkit";

import { writeLastDirectory } from "@/features/workspace/local-preferences";

import {
  abortPi,
  connectPi,
  disconnectPi,
  getAvailableModels,
  getDefaultWorkspace,
  getPiCommands,
  getPiState,
  getWorkspaceContext,
  listenToPiEvents,
  newPiSession,
  respondToExtensionUi,
  selectModel,
  selectThinkingLevel,
  sendPrompt,
} from "./api";
import type { LureError } from "./pi-session-types";
import { disconnectedSnapshot } from "./pi-session-domain";
import { piConnectionActions, type PiConnectionState } from "./pi-session-slice";

export const piSessionListenerMiddleware = createListenerMiddleware<{ piConnection: PiConnectionState }, Dispatch<UnknownAction>>();

type ListenerApi = {
  dispatch: Dispatch<UnknownAction>;
  getState: () => { piConnection: PiConnectionState };
};

let unsubscribe: (() => void) | undefined;
let subscriptionWanted = false;
let subscriptionPending = false;
let lastSequence = 0;
let startupAttempted = false;

function normalizeError(error: unknown, fallback: string): LureError {
  if (typeof error === "object" && error !== null) {
    const candidate = error as Partial<LureError>;
    if (typeof candidate.code === "string" && typeof candidate.message === "string") {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return { code: "CLIENT_ERROR", message: error instanceof Error ? error.message : fallback };
}

function reportError(api: ListenerApi, error: unknown, fallback: string) {
  api.dispatch(piConnectionActions.commandFailed(normalizeError(error, fallback)));
}

async function refreshCapabilities(api: ListenerApi) {
  const [models, commands] = await Promise.allSettled([getAvailableModels(), getPiCommands()]);
  api.dispatch(
    piConnectionActions.capabilitiesLoaded({
      models: models.status === "fulfilled" && Array.isArray(models.value) ? models.value : [],
      commands: commands.status === "fulfilled" && Array.isArray(commands.value) ? commands.value : [],
    }),
  );
}

async function loadWorkspaceContext(api: ListenerApi, directory: string) {
  try {
    const context = await getWorkspaceContext(directory);
    api.dispatch(piConnectionActions.workspaceContextLoaded(context ?? { branch: null, workingDirectory: directory }));
  } catch {
    api.dispatch(piConnectionActions.workspaceContextLoaded({ branch: null, workingDirectory: directory }));
  }
}

async function hydrateConnectedSession(api: ListenerApi, snapshot: PiConnectionState["connection"], directory: string) {
  api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "session_ready", snapshot } }));
  await Promise.all([refreshCapabilities(api), loadWorkspaceContext(api, snapshot.workingDirectory ?? directory)]);
}

async function restoreOrConnectDefault(api: ListenerApi) {
  try {
    const snapshot = await getPiState();
    if (
      (snapshot.phase === "connecting" || snapshot.phase === "ready" || snapshot.phase === "running") &&
      snapshot.workingDirectory
    ) {
      api.dispatch(piConnectionActions.selectedDirectoryChanged(snapshot.workingDirectory));
      await hydrateConnectedSession(api, snapshot, snapshot.workingDirectory);
      return true;
    }
  } catch {
    // 读取快照失败时仍尝试创建默认会话，让连接错误走既有的统一呈现路径。
  }
  return connectDefault(api);
}

async function connectDefault(api: ListenerApi) {
  if (!api.getState().piConnection.eventsReady) return false;
  api.dispatch(piConnectionActions.commandErrorCleared());
  try {
    const directory = await getDefaultWorkspace();
    api.dispatch(piConnectionActions.selectedDirectoryChanged(directory));
    writeLastDirectory(directory);
    await hydrateConnectedSession(api, await connectPi(directory), directory);
    return true;
  } catch (error) {
    reportError(api, error, "启动默认 Pi RPC 失败");
    return false;
  }
}

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.eventSubscriptionRequested,
  effect: async (_action, api) => {
    subscriptionWanted = true;
    if (unsubscribe || subscriptionPending) return;
    subscriptionPending = true;
    try {
      const dispose = await listenToPiEvents((event) => {
        if (event.sequence !== 0) {
          if (event.sequence <= lastSequence) return;
          lastSequence = event.sequence;
        }
        api.dispatch(piConnectionActions.piEventReceived(event));
      });
      subscriptionPending = false;
      if (!subscriptionWanted) dispose();
      else {
        unsubscribe = dispose;
        api.dispatch(piConnectionActions.eventsReadinessChanged(true));
        if (!startupAttempted) {
          startupAttempted = true;
          api.dispatch(piConnectionActions.startupRequested());
        }
      }
    } catch (error) {
      subscriptionPending = false;
      reportError(api, error, "无法监听 Pi RPC 事件");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.eventSubscriptionReleased,
  effect: (_action, api) => {
    subscriptionWanted = false;
    unsubscribe?.();
    unsubscribe = undefined;
    api.dispatch(piConnectionActions.eventsReadinessChanged(false));
    queueMicrotask(() => {
      if (!subscriptionWanted) {
        startupAttempted = false;
        lastSequence = 0;
        api.dispatch(piConnectionActions.sessionReset());
      }
    });
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.startupRequested,
  effect: async (_action, api) => { await restoreOrConnectDefault(api); },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.connectRequested,
  effect: async (_action, api) => {
    const { selectedDirectory, eventsReady } = api.getState().piConnection;
    if (!selectedDirectory || !eventsReady) return;
    api.dispatch(piConnectionActions.commandErrorCleared());
    try {
      await hydrateConnectedSession(api, await connectPi(selectedDirectory), selectedDirectory);
    } catch (error) {
      reportError(api, error, "连接 Pi 失败");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.retryRequested,
  effect: async (_action, api) => {
    const state = api.getState().piConnection;
    if (state.connection.phase === "failed" && state.connection.sessionId) {
      try {
        await disconnectPi();
        api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "connection_changed", snapshot: disconnectedSnapshot } }));
        api.dispatch(piConnectionActions.disconnectedCapabilitiesCleared());
      } catch (error) {
        reportError(api, error, "断开 Pi 失败");
        return;
      }
    }
    if (api.getState().piConnection.selectedDirectory) api.dispatch(piConnectionActions.connectRequested());
    else await connectDefault(api);
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.newConversationRequested,
  effect: async (_action, api) => {
    const state = api.getState().piConnection;
    if (state.connection.phase === "disconnected" || state.connection.phase === "failed") {
      await connectDefault(api);
      return;
    }
    api.dispatch(piConnectionActions.commandErrorCleared());
    try {
      const snapshot = await newPiSession();
      const directory = snapshot.workingDirectory ?? state.selectedDirectory;
      if (directory) api.dispatch(piConnectionActions.selectedDirectoryChanged(directory));
      await hydrateConnectedSession(api, snapshot, directory ?? "");
    } catch (error) {
      reportError(api, error, "新建对话失败");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.disconnectRequested,
  effect: async (_action, api) => {
    api.dispatch(piConnectionActions.commandErrorCleared());
    try {
      await disconnectPi();
      api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "connection_changed", snapshot: disconnectedSnapshot } }));
      api.dispatch(piConnectionActions.disconnectedCapabilitiesCleared());
    } catch (error) { reportError(api, error, "断开 Pi 失败"); }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.promptRequested,
  effect: async (action, api) => {
    api.dispatch(piConnectionActions.commandErrorCleared());
    try {
      await sendPrompt(action.payload.message, action.payload.images);
      api.dispatch(piConnectionActions.promptAccepted());
    }
    catch (error) { reportError(api, error, "发送消息失败"); }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.abortRequested,
  effect: async (_action, api) => { try { await abortPi(); } catch (error) { reportError(api, error, "停止运行失败"); } },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.modelSelectionRequested,
  effect: async (action, api) => {
    try {
      const model = await selectModel(action.payload.provider, action.payload.id);
      const snapshot = api.getState().piConnection.connection;
      api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "connection_changed", snapshot: { ...snapshot, model } } }));
    } catch (error) { reportError(api, error, "切换模型失败"); }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.thinkingLevelSelectionRequested,
  effect: async (action, api) => {
    try {
      const thinkingLevel = await selectThinkingLevel(action.payload);
      const snapshot = api.getState().piConnection.connection;
      api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "connection_changed", snapshot: { ...snapshot, thinkingLevel } } }));
    } catch (error) { reportError(api, error, "切换思考强度失败"); }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.extensionResponseRequested,
  effect: async (action, api) => {
    const request = api.getState().piConnection.extensionRequest;
    if (!request) return;
    try { await respondToExtensionUi(request.requestId, action.payload.value, action.payload.cancelled); }
    catch (error) { reportError(api, error, "无法提交交互响应"); }
  },
});
