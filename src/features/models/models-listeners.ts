import { createListenerMiddleware } from "@reduxjs/toolkit";

import { sessionsActions } from "@/features/sessions";
import { selectModel, selectThinkingLevel } from "@/lib/pi-rpc/client";
import { normalizeLureError } from "@/lib/normalize-error";

import { modelsActions } from "./models-slice";

export const modelsListenerMiddleware = createListenerMiddleware();

modelsListenerMiddleware.startListening({
  actionCreator: modelsActions.modelSelectionRequested,
  effect: async (action, api) => {
    try {
      const model = await selectModel(
        action.payload.provider,
        action.payload.id,
      );
      api.dispatch(modelsActions.modelSelected(model));
    } catch (error) {
      api.dispatch(
        sessionsActions.commandFailed(
          normalizeLureError(error, "切换模型失败"),
        ),
      );
    }
  },
});

modelsListenerMiddleware.startListening({
  actionCreator: modelsActions.thinkingLevelSelectionRequested,
  effect: async (action, api) => {
    try {
      api.dispatch(
        modelsActions.thinkingLevelSelected(
          await selectThinkingLevel(action.payload),
        ),
      );
    } catch (error) {
      api.dispatch(
        sessionsActions.commandFailed(
          normalizeLureError(error, "切换思考强度失败"),
        ),
      );
    }
  },
});
