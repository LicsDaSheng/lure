import { configureStore } from "@reduxjs/toolkit";
import { describe, expect, it } from "vitest";

import { selectRecentConversations } from "./pi-session-selectors";
import { piConnectionActions, piConnectionReducer } from "./pi-session-slice";

describe("Pi connection selectors", () => {
  it("按最近活动时间倒序返回对话且不改写 Redux 原始顺序", () => {
    const store = configureStore({ reducer: { piConnection: piConnectionReducer } });
    const stored = [
      { sessionId: "older", title: "较早", updatedAt: 10 },
      { sessionId: "latest", title: "最新", updatedAt: 20 },
    ];
    store.dispatch(piConnectionActions.recentConversationsLoaded(stored));

    expect(selectRecentConversations(store.getState()).map((item) => item.sessionId)).toEqual([
      "latest",
      "older",
    ]);
    expect(store.getState().piConnection.recentConversations).toEqual(stored);
  });
});
