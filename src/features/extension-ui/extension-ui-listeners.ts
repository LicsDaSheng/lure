import { createListenerMiddleware } from "@reduxjs/toolkit";

import { sessionsActions } from "@/features/sessions";
import { respondToExtensionUi } from "@/lib/pi-rpc/client";
import { normalizeLureError } from "@/lib/normalize-error";

import { extensionUiActions } from "./extension-ui-slice";

type ExtensionState = { extensionUi: { request: { requestId: string } | null } };

export const extensionUiListenerMiddleware = createListenerMiddleware<ExtensionState>();

extensionUiListenerMiddleware.startListening({
  actionCreator: extensionUiActions.responseRequested,
  effect: async (action, api) => {
    const request = api.getState().extensionUi.request;
    if (!request) return;
    try {
      await respondToExtensionUi(request.requestId, action.payload.value, action.payload.cancelled);
    } catch (error) {
      api.dispatch(sessionsActions.commandFailed(normalizeLureError(error, "无法提交交互响应")));
    }
  },
});
