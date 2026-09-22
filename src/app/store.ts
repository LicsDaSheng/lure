import { configureStore } from "@reduxjs/toolkit";

import { conversationListenerMiddleware, conversationReducer } from "@/features/conversation";
import { executionReducer } from "@/features/execution";
import { extensionUiListenerMiddleware, extensionUiReducer } from "@/features/extension-ui";
import { modelsListenerMiddleware, modelsReducer } from "@/features/models";
import { sessionsListenerMiddleware, sessionsReducer } from "@/features/sessions";

export const store = configureStore({
  reducer: {
    sessions: sessionsReducer,
    conversation: conversationReducer,
    execution: executionReducer,
    models: modelsReducer,
    extensionUi: extensionUiReducer,
  },
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware().prepend(
      sessionsListenerMiddleware.middleware,
      conversationListenerMiddleware.middleware,
      modelsListenerMiddleware.middleware,
      extensionUiListenerMiddleware.middleware,
    ),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
