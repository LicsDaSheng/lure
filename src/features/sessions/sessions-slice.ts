import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import { disconnectedSnapshot } from "@/features/conversation/public";
import type {
  ConnectionSnapshot,
  LureError,
  PiSessionSummary,
  ProjectDescriptor,
  SessionEntries,
} from "@/lib/pi-rpc/types";
import type { WorkspaceContext } from "@/lib/pi-rpc/client";

import { piRuntimeProjected } from "./runtime-events";

export type SessionConnection = Omit<ConnectionSnapshot, "model" | "thinkingLevel">;

/**
 * 历史会话切换事务：连接重建与会话切换是跨多个 RPC 调用的过程，
 * 这里只记录正在切换的目标会话（为空表示没有事务在进行）。
 */
export type SessionTransition = string | null;

export type SessionsState = {
  connection: SessionConnection;
  error: LureError | null;
  selectedDirectory: string | null;
  projectDirectoryCandidate: string | null;
  eventsReady: boolean;
  commandError: LureError | null;
  workspaceContext: WorkspaceContext | null;
  defaultWorkspace: string | null;
  projects: ProjectDescriptor[];
  recentSessions: PiSessionSummary[];
  projectSessions: Record<string, PiSessionSummary[]>;
  recentSessionsHasMore: boolean;
  projectSessionsHasMore: Record<string, boolean>;
  expandedProjects: string[];
  loadingDirectories: string[];
  sessionTransition: SessionTransition;
  /**
   * 当前打开的历史会话。它可能不在已加载的分页里（分页只取最新几条），
   * 导航需要它保持可见，所以单独保留事实并由 selector 合并进列表。
   */
  activeSessionSummary: PiSessionSummary | null;
};

function sessionConnection(snapshot: ConnectionSnapshot): SessionConnection {
  const { model: _model, thinkingLevel: _thinkingLevel, ...connection } = snapshot;
  return connection;
}

export const initialSessionsState: SessionsState = {
  connection: sessionConnection(disconnectedSnapshot),
  error: null,
  selectedDirectory: null,
  projectDirectoryCandidate: null,
  eventsReady: false,
  commandError: null,
  workspaceContext: null,
  defaultWorkspace: null,
  projects: [],
  recentSessions: [],
  projectSessions: {},
  recentSessionsHasMore: false,
  projectSessionsHasMore: {},
  expandedProjects: [],
  loadingDirectories: [],
  sessionTransition: null,
  activeSessionSummary: null,
};

const sessionsSlice = createSlice({
  name: "sessions",
  initialState: initialSessionsState,
  reducers: {
    sessionReset: () => initialSessionsState,
    eventSubscriptionRequested: () => undefined,
    eventSubscriptionReleased: () => undefined,
    startupRequested: () => undefined,
    connectRequested: () => undefined,
    retryRequested: () => undefined,
    newConversationRequested: () => undefined,
    defaultConversationRequested: () => undefined,
    projectConversationRequested: (_state, _action: PayloadAction<string>) => undefined,
    conversationOpenRequested: (_state, _action: PayloadAction<PiSessionSummary>) => undefined,
    disconnectRequested: () => undefined,
    projectDirectorySelectionRequested: () => undefined,
    projectDirectorySelected: (state, action: PayloadAction<string | null>) => {
      state.projectDirectoryCandidate = action.payload;
    },
    selectedDirectoryChanged: (state, action: PayloadAction<string>) => {
      state.selectedDirectory = action.payload;
    },
    projectCatalogLoaded: (
      state,
      action: PayloadAction<{ defaultWorkspace: string; projects: ProjectDescriptor[] }>,
    ) => {
      state.defaultWorkspace = action.payload.defaultWorkspace;
      state.projects = action.payload.projects;
    },
    projectAdded: (state, action: PayloadAction<ProjectDescriptor>) => {
      const project = action.payload;
      if (project.directory === state.defaultWorkspace) return;
      const existing = state.projects.find((item) => item.directory === project.directory);
      if (existing) existing.name = project.name;
      else state.projects.push(project);
    },
    projectExpansionToggled: (state, action: PayloadAction<string>) => {
      const directory = action.payload;
      state.expandedProjects = state.expandedProjects.includes(directory)
        ? state.expandedProjects.filter((item) => item !== directory)
        : [...state.expandedProjects, directory];
    },
    sessionPageRequested: (_state, _action: PayloadAction<string>) => undefined,
    sessionSwitchStarted: (state, action: PayloadAction<string>) => {
      state.sessionTransition = action.payload;
    },
    sessionSwitchCleared: (state) => {
      state.sessionTransition = null;
    },
    activeSessionRecorded: (state, action: PayloadAction<PiSessionSummary | null>) => {
      state.activeSessionSummary = action.payload;
    },
    sessionsRequested: (state, action: PayloadAction<string>) => {
      if (!state.loadingDirectories.includes(action.payload)) state.loadingDirectories.push(action.payload);
    },
    sessionsRequestFailed: (state, action: PayloadAction<string>) => {
      state.loadingDirectories = state.loadingDirectories.filter((item) => item !== action.payload);
    },
    sessionsLoaded: (
      state,
      action: PayloadAction<{
        directory: string;
        sessions: PiSessionSummary[];
        hasMore: boolean;
        append: boolean;
      }>,
    ) => {
      const { append, directory, hasMore, sessions } = action.payload;
      state.loadingDirectories = state.loadingDirectories.filter((item) => item !== directory);
      if (directory === state.defaultWorkspace) {
        state.recentSessions = append ? mergeSessions(state.recentSessions, sessions) : sessions;
        state.recentSessionsHasMore = hasMore;
      } else {
        const current = state.projectSessions[directory] ?? [];
        state.projectSessions[directory] = append ? mergeSessions(current, sessions) : sessions;
        state.projectSessionsHasMore[directory] = hasMore;
      }
    },
    historyLoaded: (_state, _action: PayloadAction<SessionEntries>) => undefined,
    eventsReadinessChanged: (state, action: PayloadAction<boolean>) => {
      state.eventsReady = action.payload;
    },
    workspaceContextLoaded: (state, action: PayloadAction<WorkspaceContext>) => {
      state.workspaceContext = action.payload;
    },
    commandFailed: (state, action: PayloadAction<LureError>) => {
      state.commandError = action.payload;
    },
    connectionFailed: (state, action: PayloadAction<LureError>) => {
      // 连接失败单独入槽：连接过程静默，只有它需要弹窗打扰用户。
      state.error = action.payload;
    },
    commandErrorCleared: (state) => {
      state.commandError = null;
    },
    disconnectedCapabilitiesCleared: (state) => {
      // 历史会话列表来自本地 Pi 会话文件，与当前 RPC 连接无关：
      // 重建连接期间保留已有列表与分页状态，避免导航整片清空又重新出现。
      state.loadingDirectories = [];
    },
  },
  extraReducers: (builder) => {
    builder.addCase(piRuntimeProjected, (state, action) => {
      state.connection = sessionConnection(action.payload.state.connection);
      state.error = action.payload.state.error;
    });
  },
});

export const sessionsActions = sessionsSlice.actions;
export const sessionsReducer = sessionsSlice.reducer;

function mergeSessions(current: PiSessionSummary[], next: PiSessionSummary[]) {
  const byPath = new Map(current.map((session) => [session.path, session]));
  next.forEach((session) => byPath.set(session.path, session));
  return [...byPath.values()];
}
