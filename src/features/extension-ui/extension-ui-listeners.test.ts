import { configureStore } from "@reduxjs/toolkit";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { sessionsReducer } from "@/features/sessions/sessions-slice";

const mocks = vi.hoisted(() => ({ respondToExtensionUi: vi.fn() }));
vi.mock("@/lib/pi-rpc/client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/pi-rpc/client")>()),
  ...mocks,
}));

import { extensionUiListenerMiddleware } from "./extension-ui-listeners";
import { extensionUiActions, extensionUiReducer } from "./extension-ui-slice";

function createStore(request: { requestId: string } | null) {
  return configureStore({
    reducer: { extensionUi: extensionUiReducer, sessions: sessionsReducer },
    preloadedState: { extensionUi: { request: request as never } },
    middleware: (defaults) =>
      defaults().prepend(extensionUiListenerMiddleware.middleware),
  });
}

beforeEach(() => mocks.respondToExtensionUi.mockReset());

describe("extension UI listeners", () => {
  it("把用户响应关联到当前 request id", async () => {
    mocks.respondToExtensionUi.mockResolvedValue(undefined);
    const store = createStore({ requestId: "request-1" });
    store.dispatch(
      extensionUiActions.responseRequested({ value: true, cancelled: false }),
    );
    await vi.waitFor(() =>
      expect(mocks.respondToExtensionUi).toHaveBeenCalledWith(
        "request-1",
        true,
        false,
      ),
    );
  });
});
