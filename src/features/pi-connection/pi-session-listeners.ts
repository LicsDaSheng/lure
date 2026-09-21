import { createListenerMiddleware } from "@reduxjs/toolkit";
import type { Dispatch, UnknownAction } from "@reduxjs/toolkit";

import {
  readProjects,
  readRecentConversations,
  writeLastDirectory,
  writeProjects,
  writeRecentConversations,
} from "@/features/workspace/local-preferences";

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
import type { EventEnvelope, LureError } from "./pi-session-types";
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
let streamFlushTimer: ReturnType<typeof setTimeout> | undefined;
let pendingStreamEvents: EventEnvelope[] = [];

const STREAM_FLUSH_INTERVAL_MS = 32;

function isStreamingDelta(envelope: EventEnvelope) {
  return (
    envelope.event.type === "assistant_text_delta" ||
    envelope.event.type === "assistant_thinking_delta"
  );
}

function flushStreamingEvents(api: ListenerApi) {
  if (streamFlushTimer !== undefined) {
    clearTimeout(streamFlushTimer);
    streamFlushTimer = undefined;
  }
  if (pendingStreamEvents.length === 0) return;
  const events = pendingStreamEvents;
  pendingStreamEvents = [];
  api.dispatch(piConnectionActions.piEventsReceived(events));
}

function enqueueStreamingEvent(api: ListenerApi, envelope: EventEnvelope) {
  const previous = pendingStreamEvents.at(-1);
  const event = envelope.event;
  if (
    previous &&
    (event.type === "assistant_text_delta" || event.type === "assistant_thinking_delta") &&
    previous.event.type === event.type &&
    previous.event.contentIndex === event.contentIndex
  ) {
    pendingStreamEvents[pendingStreamEvents.length - 1] = {
      sequence: envelope.sequence,
      event: { ...event, delta: previous.event.delta + event.delta },
    };
  } else {
    pendingStreamEvents.push(envelope);
  }

  streamFlushTimer ??= setTimeout(() => {
    flushStreamingEvents(api);
  }, STREAM_FLUSH_INTERVAL_MS);
}

function clearStreamingEvents() {
  if (streamFlushTimer !== undefined) clearTimeout(streamFlushTimer);
  streamFlushTimer = undefined;
  pendingStreamEvents = [];
}

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
  rememberDefaultConversation(api, snapshot, directory);
  await Promise.all([refreshCapabilities(api), loadWorkspaceContext(api, snapshot.workingDirectory ?? directory)]);
}

function directoryName(directory: string) {
  return directory.split(/[\\/]/).filter(Boolean).at(-1) ?? directory;
}

function rememberDefaultConversation(
  api: ListenerApi,
  snapshot: PiConnectionState["connection"],
  directory: string,
) {
  const state = api.getState().piConnection;
  const actualDirectory = snapshot.workingDirectory ?? directory;
  const sessionId = snapshot.sessionId;
  if (!sessionId || actualDirectory !== state.defaultWorkspace) return;
  const existing = state.recentConversations.find((item) => item.sessionId === sessionId);
  api.dispatch(piConnectionActions.recentConversationUpserted({
    sessionId,
    title: existing?.title ?? directoryName(actualDirectory),
    updatedAt: existing?.updatedAt ?? Date.now(),
  }));
}

async function initializeProjectCatalog(api: ListenerApi) {
  const current = api.getState().piConnection;
  if (current.defaultWorkspace) return current.defaultWorkspace;
  const defaultWorkspace = await getDefaultWorkspace();
  const saved = readProjects();
  const projects = saved.filter((project) => project.directory !== defaultWorkspace);
  api.dispatch(piConnectionActions.projectCatalogLoaded({ defaultWorkspace, projects }));
  api.dispatch(piConnectionActions.recentConversationsLoaded(readRecentConversations()));
  writeProjects(projects);
  return defaultWorkspace;
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
    const directory = await initializeProjectCatalog(api);
    api.dispatch(piConnectionActions.selectedDirectoryChanged(directory));
    writeLastDirectory(directory);
    await hydrateConnectedSession(api, await connectPi(directory), directory);
    return true;
  } catch (error) {
    reportError(api, error, "启动默认 Pi RPC 失败");
    return false;
  }
}

async function openProjectConversation(api: ListenerApi, directory: string) {
  const state = api.getState().piConnection;
  if (!state.eventsReady || state.connection.phase === "running" || state.connection.phase === "connecting") {
    return false;
  }
  api.dispatch(piConnectionActions.commandErrorCleared());
  try {
    const currentDirectory = state.connection.workingDirectory ?? state.selectedDirectory;
    if (currentDirectory === directory && state.connection.phase === "ready") {
      await hydrateConnectedSession(api, await newPiSession(), directory);
      return true;
    }

    if (state.connection.phase !== "disconnected") {
      await disconnectPi();
      api.dispatch(piConnectionActions.piEventReceived({
        sequence: 0,
        event: { type: "connection_changed", snapshot: disconnectedSnapshot },
      }));
      api.dispatch(piConnectionActions.disconnectedCapabilitiesCleared());
    }
    api.dispatch(piConnectionActions.selectedDirectoryChanged(directory));
    writeLastDirectory(directory);
    await hydrateConnectedSession(api, await connectPi(directory), directory);
    return true;
  } catch (error) {
    reportError(api, error, "无法在项目中创建对话");
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
        if (isStreamingDelta(event)) {
          enqueueStreamingEvent(api, event);
          return;
        }
        // 非流式事件必须排在此前已收到的增量之后，保证工具和消息完成事件的顺序。
        flushStreamingEvents(api);
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
    clearStreamingEvents();
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
  effect: async (_action, api) => {
    try {
      await initializeProjectCatalog(api);
      await restoreOrConnectDefault(api);
    } catch (error) {
      reportError(api, error, "无法加载项目导航");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.projectAdded,
  effect: (_action, api) => {
    writeProjects(api.getState().piConnection.projects);
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.recentConversationUpserted,
  effect: (_action, api) => {
    writeRecentConversations(api.getState().piConnection.recentConversations);
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.recentConversationTitleChanged,
  effect: (action, api) => {
    const state = api.getState().piConnection;
    const sessionId = state.connection.sessionId;
    const directory = state.connection.workingDirectory ?? state.selectedDirectory;
    if (!sessionId || directory !== state.defaultWorkspace || !action.payload.trim()) return;
    api.dispatch(piConnectionActions.recentConversationUpserted({
      sessionId,
      title: action.payload.trim(),
      updatedAt: Date.now(),
    }));
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.promptAccepted,
  effect: (_action, api) => {
    const state = api.getState().piConnection;
    const sessionId = state.connection.sessionId;
    const directory = state.connection.workingDirectory ?? state.selectedDirectory;
    if (!sessionId || !directory || directory !== state.defaultWorkspace) return;
    const existing = state.recentConversations.find((item) => item.sessionId === sessionId);
    api.dispatch(piConnectionActions.recentConversationUpserted({
      sessionId,
      title: existing?.title ?? directoryName(directory),
      updatedAt: Date.now(),
    }));
  },
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
  actionCreator: piConnectionActions.defaultConversationRequested,
  effect: async (_action, api) => {
    try {
      const directory = await initializeProjectCatalog(api);
      await openProjectConversation(api, directory);
    } catch (error) {
      reportError(api, error, "无法在 lure 项目中创建对话");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.projectConversationRequested,
  effect: async (action, api) => {
    await openProjectConversation(api, action.payload);
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
