import { configureStore } from "@reduxjs/toolkit";

import { piSessionListenerMiddleware } from "@/features/pi-connection/pi-session-listeners";
import { piConnectionReducer } from "@/features/pi-connection/pi-session-slice";

export const store = configureStore({
  reducer: { piConnection: piConnectionReducer },
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware().prepend(piSessionListenerMiddleware.middleware),
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
