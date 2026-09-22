import { createListenerMiddleware } from "@reduxjs/toolkit";

import { sessionsActions } from "@/features/sessions";
import { abortPi, sendPrompt } from "@/lib/pi-rpc/client";
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
