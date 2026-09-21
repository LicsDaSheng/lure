import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import type { ImageAttachment, PiCommand, WorkspaceContext } from "./api";
import {
  type EventEnvelope,
  type LureError,
  type ModelSnapshot,
  type PiSessionState,
} from "./pi-session-types";
import { initialPiSessionState, piSessionReducer } from "./pi-session-domain";

export type PiConnectionState = PiSessionState & {
  selectedDirectory: string | null;
  eventsReady: boolean;
  commandError: LureError | null;
  availableModels: ModelSnapshot[];
  commands: PiCommand[];
  workspaceContext: WorkspaceContext | null;
  promptSubmissionCount: number;
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
    selectedDirectoryChanged: (state, action: PayloadAction<string>) => {
      state.selectedDirectory = action.payload;
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
    },
  },
});

export const piConnectionActions = piConnectionSlice.actions;
export const piConnectionReducer = piConnectionSlice.reducer;
