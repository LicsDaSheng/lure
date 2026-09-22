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
  /**
   * 历史会话切换事务期间保留当前对话：Pi 报告新会话就绪时先缓存其消息，
   * 直到事务结束（历史条目提交或切换失败）才决定是否替换当前内容。
   */
  switchDeferred: boolean;
  deferredMessages: ConversationMessage[] | null;
};

export const initialConversationState: ConversationState = {
  messages: [],
  activeAssistantId: null,
  promptSubmissionCount: 0,
  commands: [],
  switchDeferred: false,
  deferredMessages: null,
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
      state.deferredMessages = null;
    },
    /** 切换事务开始：暂缓把新会话的报告内容替换到当前对话上。 */
    sessionSwitchDeferred: (state) => {
      state.switchDeferred = true;
      state.deferredMessages = null;
    },
    /** 切换事务结束：未提交历史条目时采用 Pi 报告的最新对话内容。 */
    sessionSwitchSettled: (state) => {
      state.switchDeferred = false;
      if (state.deferredMessages) state.messages = state.deferredMessages;
      state.deferredMessages = null;
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
      const next = action.payload.state.messages;
      state.activeAssistantId = action.payload.state.activeAssistantId;
      if (state.switchDeferred) {
        state.deferredMessages = next;
        return;
      }
      state.messages = next;
      state.deferredMessages = null;
    });
  },
});

export const conversationActions = conversationSlice.actions;
export const conversationReducer = conversationSlice.reducer;
