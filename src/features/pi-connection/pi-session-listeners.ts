import { createListenerMiddleware } from "@reduxjs/toolkit";
import type { Dispatch, UnknownAction } from "@reduxjs/toolkit";

import {
  readProjects,
  writeLastDirectory,
  writeProjects,
} from "@/features/workspace/local-preferences";

import {
  abortPi,
  connectPi,
  disconnectPi,
  getAvailableModels,
  getDefaultWorkspace,
  getPiCommands,
  getPiState,
  getSessionEntries,
  getWorkspaceContext,
  listProjectSessions,
  listenToPiEvents,
  newPiSession,
  respondToExtensionUi,
  selectModel,
  selectThinkingLevel,
  sendPrompt,
  switchPiSession,
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
const INITIAL_SESSION_PAGE_SIZE = 3;
const SESSION_PAGE_SIZE = 5;

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

async function loadSessionsFor(
  api: ListenerApi,
  directory: string | null,
  { append = false, limit = INITIAL_SESSION_PAGE_SIZE }: { append?: boolean; limit?: number } = {},
) {
  if (!directory) return;
  const state = api.getState().piConnection;
  const offset = append
    ? directory === state.defaultWorkspace
      ? state.recentSessions.length
      : (state.projectSessions[directory] ?? []).length
    : 0;
  api.dispatch(piConnectionActions.sessionsRequested(directory));
  try {
    const page = await listProjectSessions(directory, offset, limit);
    api.dispatch(piConnectionActions.sessionsLoaded({
      append,
      directory,
      hasMore: Boolean(page?.hasMore),
      sessions: Array.isArray(page?.sessions) ? page.sessions : [],
    }));
  } catch (error) {
    // 读取失败不清空已有页或 hasMore，用户可再次点击重试。
    api.dispatch(piConnectionActions.sessionsRequestFailed(directory));
    reportError(api, error, "读取历史会话失败");
  }
}

/** 刷新默认工作目录的“最近”与所有已展开项目的会话列表。 */
async function refreshSessionLists(api: ListenerApi) {
  const state = api.getState().piConnection;
  const directories: (string | null)[] = [state.defaultWorkspace, ...state.expandedProjects];
  await Promise.all(directories.map((directory) => loadSessionsFor(api, directory)));
}

async function hydrateConnectedSession(api: ListenerApi, snapshot: PiConnectionState["connection"], directory: string) {
  api.dispatch(piConnectionActions.piEventReceived({ sequence: 0, event: { type: "session_ready", snapshot } }));
  const activeDirectory = snapshot.workingDirectory ?? directory;
  await Promise.all([
    refreshCapabilities(api),
    loadWorkspaceContext(api, activeDirectory),
    refreshSessionLists(api),
  ]);
}

async function initializeProjectCatalog(api: ListenerApi) {
  const current = api.getState().piConnection;
  if (current.defaultWorkspace) return current.defaultWorkspace;
  const defaultWorkspace = await getDefaultWorkspace();
  const saved = readProjects();
  const projects = saved.filter((project) => project.directory !== defaultWorkspace);
  api.dispatch(piConnectionActions.projectCatalogLoaded({ defaultWorkspace, projects }));
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
  actionCreator: piConnectionActions.projectExpansionToggled,
  effect: async (action, api) => {
    // 折叠时不重复查询，展开时才读取该项目最新的历史会话。
    if (!api.getState().piConnection.expandedProjects.includes(action.payload)) return;
    await loadSessionsFor(api, action.payload);
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.sessionPageRequested,
  effect: async (action, api) => {
    if (api.getState().piConnection.loadingDirectories.includes(action.payload)) return;
    await loadSessionsFor(api, action.payload, { append: true, limit: SESSION_PAGE_SIZE });
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.conversationOpenRequested,
  effect: async (action, api) => {
    const session = action.payload;
    const state = api.getState().piConnection;
    if (state.connection.phase !== "ready") return;

    const currentDirectory = state.connection.workingDirectory ?? state.selectedDirectory;
    const targetDirectory = session.cwd ?? currentDirectory;
    if (!targetDirectory) return;
    api.dispatch(piConnectionActions.commandErrorCleared());

    try {
      if (currentDirectory !== targetDirectory) {
        // 会话属于另一个工作目录：Pi 必须在该目录下运行，先重建 RPC 进程。
        await disconnectPi();
        api.dispatch(piConnectionActions.piEventReceived({
          sequence: 0,
          event: { type: "connection_changed", snapshot: disconnectedSnapshot },
        }));
        api.dispatch(piConnectionActions.disconnectedCapabilitiesCleared());
        api.dispatch(piConnectionActions.selectedDirectoryChanged(targetDirectory));
        writeLastDirectory(targetDirectory);
        await hydrateConnectedSession(api, await connectPi(targetDirectory), targetDirectory);
      }

      const outcome = await switchPiSession(session.path);
      if (!outcome.switched) {
        api.dispatch(piConnectionActions.commandFailed({
          code: "SESSION_SWITCH_CANCELLED",
          message: "Pi 扩展取消了这次会话切换，当前对话保持不变。",
        }));
        return;
      }
      const activeDirectory = outcome.snapshot.workingDirectory ?? targetDirectory;
      await hydrateConnectedSession(api, outcome.snapshot, activeDirectory);
      api.dispatch(piConnectionActions.historyLoaded(await getSessionEntries()));
    } catch (error) {
      reportError(api, error, "切换到历史会话失败");
    }
  },
});

piSessionListenerMiddleware.startListening({
  actionCreator: piConnectionActions.promptAccepted,
  effect: async (_action, api) => {
    const state = api.getState().piConnection;
    const sessionId = state.connection.sessionId;
    const directory = state.connection.workingDirectory ?? state.selectedDirectory;
    if (!sessionId || !directory) return;
    // 会话文件要在第一条消息写入后才落盘，所以只在它尚未出现在列表时补一次扫描。
    const known = directory === state.defaultWorkspace
      ? state.recentSessions.some((session) => session.id === sessionId)
      : (state.projectSessions[directory] ?? []).some((session) => session.id === sessionId);
    if (known) return;
    await loadSessionsFor(api, directory);
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
