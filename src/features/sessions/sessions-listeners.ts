import { createListenerMiddleware } from "@reduxjs/toolkit";
import type { Dispatch, UnknownAction } from "@reduxjs/toolkit";

import { conversationActions, disconnectedSnapshot, piSessionReducer, type ConversationState } from "@/features/conversation/public";
import { executionActions, type ExecutionState } from "@/features/execution/public";
import { extensionUiActions, type ExtensionUiState } from "@/features/extension-ui/public";
import { modelsActions, type ModelsState } from "@/features/models/public";
import {
  readProjects,
  writeLastDirectory,
  writeProjects,
} from "@/features/sessions/sessions-preferences";

import {
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
  selectProjectDirectory,
  switchPiSession,
} from "@/lib/pi-rpc/client";
import type { ConnectionSnapshot, EventEnvelope, PiSessionState } from "@/lib/pi-rpc/types";
import { normalizeLureError } from "@/lib/normalize-error";

import { piRuntimeProjected } from "./runtime-events";
import { sessionsActions, type SessionsState } from "./sessions-slice";

type FeatureState = {
  sessions: SessionsState;
  conversation: ConversationState;
  execution: ExecutionState;
  models: ModelsState;
  extensionUi: ExtensionUiState;
};

export const sessionsListenerMiddleware = createListenerMiddleware<FeatureState, Dispatch<UnknownAction>>();

type ListenerApi = {
  dispatch: Dispatch<UnknownAction>;
  getState: () => FeatureState;
};

const sessionFlowActions = {
  ...sessionsActions,
  promptAccepted: conversationActions.promptAccepted,
  historyLoaded: conversationActions.historyLoaded,
};

function runtimeState(state: FeatureState): PiSessionState {
  return {
    connection: {
      ...state.sessions.connection,
      model: state.models.current,
      thinkingLevel: state.models.thinkingLevel,
    },
    messages: state.conversation.messages,
    activeAssistantId: state.conversation.activeAssistantId,
    error: state.sessions.error,
    notice: state.execution.notice,
    diagnostics: state.execution.diagnostics,
    run: state.execution.run,
    extensionRequest: state.extensionUi.request,
  };
}

function legacyState(state: FeatureState) {
  return {
    ...state.sessions,
    ...state.conversation,
    ...state.execution,
    availableModels: state.models.availableModels,
    extensionRequest: state.extensionUi.request,
    connection: runtimeState(state).connection,
  };
}

function projectRuntimeEvents(api: ListenerApi, envelopes: EventEnvelope[]) {
  const projected = envelopes.reduce(piSessionReducer, runtimeState(api.getState()));
  api.dispatch(piRuntimeProjected({ envelopes, state: projected }));
}

function clearCapabilities(api: ListenerApi) {
  api.dispatch(sessionsActions.disconnectedCapabilitiesCleared());
  api.dispatch(modelsActions.capabilitiesCleared());
  api.dispatch(conversationActions.capabilitiesCleared());
}

function resetFeatures(api: ListenerApi) {
  api.dispatch(sessionsActions.sessionReset());
  api.dispatch(conversationActions.conversationReset());
  api.dispatch(executionActions.executionReset());
  api.dispatch(modelsActions.modelsReset());
  api.dispatch(extensionUiActions.extensionUiReset());
}

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
  projectRuntimeEvents(api, events);
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

function reportError(api: ListenerApi, error: unknown, fallback: string) {
  api.dispatch(sessionFlowActions.commandFailed(normalizeLureError(error, fallback)));
}

async function refreshCapabilities(api: ListenerApi) {
  const [models, commands] = await Promise.allSettled([getAvailableModels(), getPiCommands()]);
  api.dispatch(modelsActions.modelsLoaded(
    models.status === "fulfilled" && Array.isArray(models.value) ? models.value : [],
  ));
  api.dispatch(conversationActions.commandsLoaded(
    commands.status === "fulfilled" && Array.isArray(commands.value) ? commands.value : [],
  ));
}

async function loadWorkspaceContext(api: ListenerApi, directory: string) {
  try {
    const context = await getWorkspaceContext(directory);
    api.dispatch(sessionFlowActions.workspaceContextLoaded(context ?? { branch: null, workingDirectory: directory }));
  } catch {
    api.dispatch(sessionFlowActions.workspaceContextLoaded({ branch: null, workingDirectory: directory }));
  }
}

async function loadSessionsFor(
  api: ListenerApi,
  directory: string | null,
  { append = false, limit = INITIAL_SESSION_PAGE_SIZE }: { append?: boolean; limit?: number } = {},
) {
  if (!directory) return;
  const state = legacyState(api.getState());
  const offset = append
    ? directory === state.defaultWorkspace
      ? state.recentSessions.length
      : (state.projectSessions[directory] ?? []).length
    : 0;
  api.dispatch(sessionFlowActions.sessionsRequested(directory));
  try {
    const page = await listProjectSessions(directory, offset, limit);
    api.dispatch(sessionFlowActions.sessionsLoaded({
      append,
      directory,
      hasMore: Boolean(page?.hasMore),
      sessions: Array.isArray(page?.sessions) ? page.sessions : [],
    }));
  } catch (error) {
    // 读取失败不清空已有页或 hasMore，用户可再次点击重试。
    api.dispatch(sessionFlowActions.sessionsRequestFailed(directory));
    reportError(api, error, "读取历史会话失败");
  }
}

/** 刷新默认工作目录的“最近”与所有已展开项目的会话列表。 */
async function refreshSessionLists(api: ListenerApi) {
  const state = legacyState(api.getState());
  const directories: (string | null)[] = [state.defaultWorkspace, ...state.expandedProjects];
  await Promise.all(directories.map((directory) => loadSessionsFor(api, directory)));
}

async function hydrateConnectedSession(api: ListenerApi, snapshot: ConnectionSnapshot, directory: string) {
  projectRuntimeEvents(api, [{ sequence: 0, event: { type: "session_ready", snapshot } }]);
  const activeDirectory = snapshot.workingDirectory ?? directory;
  await Promise.all([
    refreshCapabilities(api),
    loadWorkspaceContext(api, activeDirectory),
    refreshSessionLists(api),
  ]);
}

async function initializeProjectCatalog(api: ListenerApi) {
  const current = legacyState(api.getState());
  if (current.defaultWorkspace) return current.defaultWorkspace;
  const defaultWorkspace = await getDefaultWorkspace();
  const saved = readProjects();
  const projects = saved.filter((project) => project.directory !== defaultWorkspace);
  api.dispatch(sessionFlowActions.projectCatalogLoaded({ defaultWorkspace, projects }));
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
      api.dispatch(sessionFlowActions.selectedDirectoryChanged(snapshot.workingDirectory));
      await hydrateConnectedSession(api, snapshot, snapshot.workingDirectory);
      return true;
    }
  } catch {
    // 读取快照失败时仍尝试创建默认会话，让连接错误走既有的统一呈现路径。
  }
  return connectDefault(api);
}

async function connectDefault(api: ListenerApi) {
  if (!legacyState(api.getState()).eventsReady) return false;
  api.dispatch(sessionFlowActions.commandErrorCleared());
  try {
    const directory = await initializeProjectCatalog(api);
    api.dispatch(sessionFlowActions.selectedDirectoryChanged(directory));
    writeLastDirectory(directory);
    await hydrateConnectedSession(api, await connectPi(directory), directory);
    return true;
  } catch (error) {
    reportError(api, error, "启动默认 Pi RPC 失败");
    return false;
  }
}

async function openProjectConversation(api: ListenerApi, directory: string) {
  const state = legacyState(api.getState());
  if (!state.eventsReady || state.connection.phase === "running" || state.connection.phase === "connecting") {
    return false;
  }
  api.dispatch(sessionFlowActions.commandErrorCleared());
  try {
    const currentDirectory = state.connection.workingDirectory ?? state.selectedDirectory;
    if (currentDirectory === directory && state.connection.phase === "ready") {
      await hydrateConnectedSession(api, await newPiSession(), directory);
      return true;
    }

    if (state.connection.phase !== "disconnected") {
      await disconnectPi();
      projectRuntimeEvents(api, [{
        sequence: 0,
        event: { type: "connection_changed", snapshot: disconnectedSnapshot },
      }]);
      clearCapabilities(api);
    }
    api.dispatch(sessionFlowActions.selectedDirectoryChanged(directory));
    writeLastDirectory(directory);
    await hydrateConnectedSession(api, await connectPi(directory), directory);
    return true;
  } catch (error) {
    reportError(api, error, "无法在项目中创建对话");
    return false;
  }
}

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.eventSubscriptionRequested,
  effect: async (_action, api) => {
    subscriptionWanted = true;
    if (unsubscribe || subscriptionPending) return;
    subscriptionPending = true;
    try {
      const dispose = await listenToPiEvents((event) => {
        const receivedEvent = { ...event, receivedAtMs: Date.now() };
        if (event.sequence !== 0) {
          if (event.sequence <= lastSequence) return;
          lastSequence = event.sequence;
        }
        if (isStreamingDelta(event)) {
          enqueueStreamingEvent(api, receivedEvent);
          return;
        }
        // 非流式事件必须排在此前已收到的增量之后，保证工具和消息完成事件的顺序。
        flushStreamingEvents(api);
        projectRuntimeEvents(api, [receivedEvent]);
      });
      subscriptionPending = false;
      if (!subscriptionWanted) dispose();
      else {
        unsubscribe = dispose;
        api.dispatch(sessionFlowActions.eventsReadinessChanged(true));
        if (!startupAttempted) {
          startupAttempted = true;
          api.dispatch(sessionFlowActions.startupRequested());
        }
      }
    } catch (error) {
      subscriptionPending = false;
      reportError(api, error, "无法监听 Pi RPC 事件");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.eventSubscriptionReleased,
  effect: (_action, api) => {
    subscriptionWanted = false;
    unsubscribe?.();
    unsubscribe = undefined;
    clearStreamingEvents();
    api.dispatch(sessionFlowActions.eventsReadinessChanged(false));
    queueMicrotask(() => {
      if (!subscriptionWanted) {
        startupAttempted = false;
        lastSequence = 0;
        resetFeatures(api);
      }
    });
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.startupRequested,
  effect: async (_action, api) => {
    try {
      await initializeProjectCatalog(api);
      await restoreOrConnectDefault(api);
    } catch (error) {
      reportError(api, error, "无法加载项目导航");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.projectAdded,
  effect: (_action, api) => {
    writeProjects(legacyState(api.getState()).projects);
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.projectDirectorySelectionRequested,
  effect: async (_action, api) => {
    try {
      api.dispatch(sessionFlowActions.projectDirectorySelected(await selectProjectDirectory()));
    } catch (error) {
      reportError(api, error, "无法选择项目目录");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.projectExpansionToggled,
  effect: async (action, api) => {
    // 折叠时不重复查询，展开时才读取该项目最新的历史会话。
    if (!legacyState(api.getState()).expandedProjects.includes(action.payload)) return;
    await loadSessionsFor(api, action.payload);
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.sessionPageRequested,
  effect: async (action, api) => {
    if (legacyState(api.getState()).loadingDirectories.includes(action.payload)) return;
    await loadSessionsFor(api, action.payload, { append: true, limit: SESSION_PAGE_SIZE });
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.conversationOpenRequested,
  effect: async (action, api) => {
    const session = action.payload;
    const state = legacyState(api.getState());
    if (state.connection.phase !== "ready") return;

    const currentDirectory = state.connection.workingDirectory ?? state.selectedDirectory;
    const targetDirectory = session.cwd ?? currentDirectory;
    if (!targetDirectory) return;
    api.dispatch(sessionFlowActions.commandErrorCleared());

    try {
      if (currentDirectory !== targetDirectory) {
        // 会话属于另一个工作目录：Pi 必须在该目录下运行，先重建 RPC 进程。
        await disconnectPi();
        projectRuntimeEvents(api, [{
          sequence: 0,
          event: { type: "connection_changed", snapshot: disconnectedSnapshot },
        }]);
        clearCapabilities(api);
        api.dispatch(sessionFlowActions.selectedDirectoryChanged(targetDirectory));
        writeLastDirectory(targetDirectory);
        await hydrateConnectedSession(api, await connectPi(targetDirectory), targetDirectory);
      }

      const outcome = await switchPiSession(session.path);
      if (!outcome.switched) {
        api.dispatch(sessionFlowActions.commandFailed({
          code: "SESSION_SWITCH_CANCELLED",
          message: "Pi 扩展取消了这次会话切换，当前对话保持不变。",
        }));
        return;
      }
      const activeDirectory = outcome.snapshot.workingDirectory ?? targetDirectory;
      await hydrateConnectedSession(api, outcome.snapshot, activeDirectory);
      api.dispatch(sessionFlowActions.historyLoaded(await getSessionEntries()));
    } catch (error) {
      reportError(api, error, "切换到历史会话失败");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.promptAccepted,
  effect: async (_action, api) => {
    const state = legacyState(api.getState());
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

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.connectRequested,
  effect: async (_action, api) => {
    const { selectedDirectory, eventsReady } = legacyState(api.getState());
    if (!selectedDirectory || !eventsReady) return;
    api.dispatch(sessionFlowActions.commandErrorCleared());
    try {
      await hydrateConnectedSession(api, await connectPi(selectedDirectory), selectedDirectory);
    } catch (error) {
      reportError(api, error, "连接 Pi 失败");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.retryRequested,
  effect: async (_action, api) => {
    const state = legacyState(api.getState());
    if (state.connection.phase === "failed" && state.connection.sessionId) {
      try {
        await disconnectPi();
        projectRuntimeEvents(api, [{ sequence: 0, event: { type: "connection_changed", snapshot: disconnectedSnapshot } }]);
        clearCapabilities(api);
      } catch (error) {
        reportError(api, error, "断开 Pi 失败");
        return;
      }
    }
    if (legacyState(api.getState()).selectedDirectory) api.dispatch(sessionFlowActions.connectRequested());
    else await connectDefault(api);
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.newConversationRequested,
  effect: async (_action, api) => {
    const state = legacyState(api.getState());
    if (state.connection.phase === "disconnected" || state.connection.phase === "failed") {
      await connectDefault(api);
      return;
    }
    api.dispatch(sessionFlowActions.commandErrorCleared());
    try {
      const snapshot = await newPiSession();
      const directory = snapshot.workingDirectory ?? state.selectedDirectory;
      if (directory) api.dispatch(sessionFlowActions.selectedDirectoryChanged(directory));
      await hydrateConnectedSession(api, snapshot, directory ?? "");
    } catch (error) {
      reportError(api, error, "新建对话失败");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.defaultConversationRequested,
  effect: async (_action, api) => {
    try {
      const directory = await initializeProjectCatalog(api);
      await openProjectConversation(api, directory);
    } catch (error) {
      reportError(api, error, "无法在 lure 项目中创建对话");
    }
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.projectConversationRequested,
  effect: async (action, api) => {
    await openProjectConversation(api, action.payload);
  },
});

sessionsListenerMiddleware.startListening({
  actionCreator: sessionFlowActions.disconnectRequested,
  effect: async (_action, api) => {
    api.dispatch(sessionFlowActions.commandErrorCleared());
    try {
      await disconnectPi();
      projectRuntimeEvents(api, [{ sequence: 0, event: { type: "connection_changed", snapshot: disconnectedSnapshot } }]);
      clearCapabilities(api);
    } catch (error) { reportError(api, error, "断开 Pi 失败"); }
  },
});
