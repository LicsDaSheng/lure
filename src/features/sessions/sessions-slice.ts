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
    commandErrorCleared: (state) => {
      state.commandError = null;
    },
    disconnectedCapabilitiesCleared: (state) => {
      state.recentSessions = [];
      state.projectSessions = {};
      state.recentSessionsHasMore = false;
      state.projectSessionsHasMore = {};
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
