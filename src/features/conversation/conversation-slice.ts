import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import { conversationFromEntries } from "./conversation-domain";
import { piRuntimeProjected } from "@/features/sessions/runtime-events";
import type { ImageAttachment, PiCommand } from "@/lib/pi-rpc/client";
import type { ConversationMessage, SessionEntries } from "@/lib/pi-rpc/types";

export type ConversationState = {
  messages: ConversationMessage[];
  activeAssistantId: string | null;
  promptSubmissionCount: number;
  commands: PiCommand[];
};

export const initialConversationState: ConversationState = {
  messages: [],
  activeAssistantId: null,
  promptSubmissionCount: 0,
  commands: [],
};

const conversationSlice = createSlice({
  name: "conversation",
  initialState: initialConversationState,
  reducers: {
    conversationReset: () => initialConversationState,
    promptRequested: (_state, _action: PayloadAction<{ message: string; images: ImageAttachment[] }>) => undefined,
    promptAccepted: (state) => {
      state.promptSubmissionCount += 1;
    },
    abortRequested: () => undefined,
    historyLoaded: (state, action: PayloadAction<SessionEntries>) => {
      state.messages = conversationFromEntries(action.payload.entries, action.payload.leafId);
      state.activeAssistantId = null;
    },
    commandsLoaded: (state, action: PayloadAction<PiCommand[]>) => {
      state.commands = action.payload;
    },
    capabilitiesCleared: (state) => {
      state.commands = [];
    },
  },
  extraReducers: (builder) => {
    builder.addCase(piRuntimeProjected, (state, action) => {
      state.messages = action.payload.state.messages;
      state.activeAssistantId = action.payload.state.activeAssistantId;
    });
  },
});

export const conversationActions = conversationSlice.actions;
export const conversationReducer = conversationSlice.reducer;
