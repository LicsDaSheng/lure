import { createListenerMiddleware } from "@reduxjs/toolkit";

import { sessionsActions } from "@/features/sessions";
import { abortPi, clearPiQueue, followUpPi, sendPrompt, steerPi } from "@/lib/pi-rpc/client";
import { normalizeLureError } from "@/lib/normalize-error";

import { conversationActions } from "./conversation-slice";

export const conversationListenerMiddleware = createListenerMiddleware();

conversationListenerMiddleware.startListening({
  actionCreator: conversationActions.promptRequested,
  effect: async (action, api) => {
    api.dispatch(sessionsActions.commandErrorCleared());
    try {
      await sendPrompt(action.payload.message, action.payload.images);
      api.dispatch(conversationActions.promptAccepted());
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "发送消息失败")));
    }
  },
});

conversationListenerMiddleware.startListening({
  actionCreator: conversationActions.abortRequested,
  effect: async (_action, api) => {
    try {
      await abortPi();
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "停止运行失败")));
    }
  },
});

conversationListenerMiddleware.startListening({
  actionCreator: conversationActions.steerRequested,
  effect: async (action, api) => {
    api.dispatch(sessionsActions.commandErrorCleared());
    try {
      await steerPi(action.payload.message, action.payload.images);
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "插队引导失败")));
    }
  },
});

conversationListenerMiddleware.startListening({
  actionCreator: conversationActions.followUpRequested,
  effect: async (action, api) => {
    api.dispatch(sessionsActions.commandErrorCleared());
    try {
      await followUpPi(action.payload.message, action.payload.images);
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "排队发送失败")));
    }
  },
});

conversationListenerMiddleware.startListening({
  actionCreator: conversationActions.queueClearRequested,
  effect: async (_action, api) => {
    api.dispatch(sessionsActions.commandErrorCleared());
    try {
      await clearPiQueue();
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "清空队列失败")));
    }
  },
});
