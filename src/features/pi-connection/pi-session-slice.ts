import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import type { ImageAttachment, PiCommand, WorkspaceContext } from "./api";
import {
  type EventEnvelope,
  type LureError,
  type ModelSnapshot,
  type PiSessionState,
  type PiSessionSummary,
  type ProjectDescriptor,
  type SessionEntries,
} from "./pi-session-types";
import { conversationFromEntries, initialPiSessionState, piSessionReducer } from "./pi-session-domain";

export type PiConnectionState = PiSessionState & {
  selectedDirectory: string | null;
  eventsReady: boolean;
  commandError: LureError | null;
  availableModels: ModelSnapshot[];
  commands: PiCommand[];
  workspaceContext: WorkspaceContext | null;
  promptSubmissionCount: number;
  defaultWorkspace: string | null;
  projects: ProjectDescriptor[];
  /** 默认工作目录中 Pi 已记录的会话。 */
  recentSessions: PiSessionSummary[];
  /** 已展开的项目目录 → 该项目目录中 Pi 已记录的会话。 */
  projectSessions: Record<string, PiSessionSummary[]>;
  /** 默认工作目录是否还有未加载的历史会话。 */
  recentSessionsHasMore: boolean;
  /** 项目目录 → 是否还有未加载的历史会话。 */
  projectSessionsHasMore: Record<string, boolean>;
  /** 用户展开的项目目录。 */
  expandedProjects: string[];
  /** 正在读取会话的目录。 */
  loadingDirectories: string[];
};

export const initialPiConnectionState: PiConnectionState = {
  ...initialPiSessionState,
  selectedDirectory: null,
  eventsReady: false,
  commandError: null,
  availableModels: [],
  commands: [],
  workspaceContext: null,
  promptSubmissionCount: 0,
  defaultWorkspace: null,
  projects: [],
  recentSessions: [],
  projectSessions: {},
  recentSessionsHasMore: false,
  projectSessionsHasMore: {},
  expandedProjects: [],
  loadingDirectories: [],
};

const piConnectionSlice = createSlice({
  name: "piConnection",
  initialState: initialPiConnectionState,
  reducers: {
    sessionReset: () => initialPiConnectionState,
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
    promptRequested: (_state, _action: PayloadAction<{ message: string; images: ImageAttachment[] }>) => undefined,
    promptAccepted: (state) => {
      state.promptSubmissionCount += 1;
    },
    abortRequested: () => undefined,
    modelSelectionRequested: (_state, _action: PayloadAction<ModelSnapshot>) => undefined,
    thinkingLevelSelectionRequested: (_state, _action: PayloadAction<string>) => undefined,
    extensionResponseRequested: (
      _state,
      _action: PayloadAction<{ value: unknown; cancelled: boolean }>,
    ) => undefined,
    piEventReceived: (state, action: PayloadAction<EventEnvelope>) => ({
      ...state,
      ...piSessionReducer(state, action.payload),
    }),
    piEventsReceived: (state, action: PayloadAction<EventEnvelope[]>) =>
      action.payload.reduce<PiConnectionState>(
        (nextState, envelope) => ({
          ...nextState,
          ...piSessionReducer(nextState, envelope),
        }),
        state as PiConnectionState,
      ),
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
    /** 展开或折叠一个项目。展开后的会话查询由 listener 触发。 */
    projectExpansionToggled: (state, action: PayloadAction<string>) => {
      const directory = action.payload;
      state.expandedProjects = state.expandedProjects.includes(directory)
        ? state.expandedProjects.filter((item) => item !== directory)
        : [...state.expandedProjects, directory];
    },
    sessionPageRequested: (_state, _action: PayloadAction<string>) => undefined,
    sessionsRequested: (state, action: PayloadAction<string>) => {
      if (!state.loadingDirectories.includes(action.payload)) {
        state.loadingDirectories.push(action.payload);
      }
    },
    sessionsRequestFailed: (state, action: PayloadAction<string>) => {
      state.loadingDirectories = state.loadingDirectories.filter((item) => item !== action.payload);
    },
    /** 默认工作目录的会话进入“最近”，其余目录的会话保存在对应项目下。 */
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
    /** 用 Pi 的会话条目重建历史对话，作为切换会话后的可追溯事实。 */
    historyLoaded: (state, action: PayloadAction<SessionEntries>) => {
      state.messages = conversationFromEntries(action.payload.entries, action.payload.leafId);
      state.activeAssistantId = null;
    },
    eventsReadinessChanged: (state, action: PayloadAction<boolean>) => {
      state.eventsReady = action.payload;
    },
    capabilitiesLoaded: (
      state,
      action: PayloadAction<{ models: ModelSnapshot[]; commands: PiCommand[] }>,
    ) => {
      state.availableModels = action.payload.models;
      state.commands = action.payload.commands;
    },
    workspaceContextLoaded: (state, action: PayloadAction<WorkspaceContext>) => {
      state.workspaceContext = action.payload;
    },
    commandFailed: (state, action: PayloadAction<LureError>) => {
      state.commandError = action.payload;
    },
    commandErrorCleared: (state) => {
      state.commandError = null;
    },
    disconnectedCapabilitiesCleared: (state) => {
      state.availableModels = [];
      state.commands = [];
      state.recentSessions = [];
      state.projectSessions = {};
      state.recentSessionsHasMore = false;
      state.projectSessionsHasMore = {};
      state.loadingDirectories = [];
    },
  },
});

export const piConnectionActions = piConnectionSlice.actions;
export const piConnectionReducer = piConnectionSlice.reducer;

function mergeSessions(current: PiSessionSummary[], next: PiSessionSummary[]) {
  const byPath = new Map(current.map((session) => [session.path, session]));
  next.forEach((session) => byPath.set(session.path, session));
  return [...byPath.values()];
}
