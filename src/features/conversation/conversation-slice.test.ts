import { describe, expect, it } from "vitest";

import type { EventEnvelope, PiSessionState } from "@/lib/pi-rpc/types";
import { piRuntimeProjected } from "@/features/sessions/runtime-events";

import { initialPiSessionState, piSessionReducer } from "./conversation-domain";
import { conversationActions, conversationReducer, initialConversationState } from "./conversation-slice";

function project(state: PiSessionState, envelopes: EventEnvelope[]): PiSessionState {
  return envelopes.reduce(piSessionReducer, state);
}

const sessionReady: EventEnvelope = {
  sequence: 0,
  event: {
    type: "session_ready",
    snapshot: {
      error: null,
      model: null,
      phase: "ready",
      sessionFile: "/tmp/lure/sessions/target.jsonl",
      sessionId: "session-target",
      thinkingLevel: "medium",
      workingDirectory: "/tmp/lure",
    },
  },
};

const userMessage: EventEnvelope = {
  sequence: 1,
  event: { type: "user_message_accepted", requestId: "req-1", message: "当前对话" },
};

describe("conversation slice 的会话切换事务", () => {
  it("非切换期间直接采用 Pi 报告的对话内容", () => {
    const piState = project(initialPiSessionState, [userMessage]);
    const next = conversationReducer(
      initialConversationState,
      piRuntimeProjected({ envelopes: [userMessage], state: piState }),
    );

    expect(next.messages).toHaveLength(1);
    expect(next.deferredMessages).toBeNull();
  });

  it("切换事务期间保留当前对话，并缓存 Pi 报告的新会话内容", () => {
    const held = conversationReducer(initialConversationState, conversationActions.sessionSwitchDeferred());
    const withCurrent = { ...held, messages: project(initialPiSessionState, [userMessage]).messages };

    const piState = project(initialPiSessionState, [sessionReady]);
    const next = conversationReducer(
      withCurrent,
      piRuntimeProjected({ envelopes: [sessionReady], state: piState }),
    );

    expect(next.messages).toEqual(withCurrent.messages);
    expect(next.deferredMessages).toEqual([]);

    // 事务结束时才把 Pi 报告的最新内容应用到当前对话。
    const settled = conversationReducer(next, conversationActions.sessionSwitchSettled());
    expect(settled.switchDeferred).toBe(false);
    expect(settled.messages).toEqual([]);
    expect(settled.deferredMessages).toBeNull();
  });

  it("切换事务提交历史条目后不再退回缓存的 Pi 内容", () => {
    const held = conversationReducer(initialConversationState, conversationActions.sessionSwitchDeferred());
    const withCache = conversationReducer(
      held,
      piRuntimeProjected({
        envelopes: [sessionReady],
        state: project(initialPiSessionState, [sessionReady]),
      }),
    );

    const committed = conversationReducer(
      withCache,
      conversationActions.historyLoaded({
        entries: [
          { type: "message", id: "e1", parentId: null, message: { role: "user", content: "历史提问" } },
        ],
        leafId: "e1",
      }),
    );
    const settled = conversationReducer(committed, conversationActions.sessionSwitchSettled());

    expect(settled.messages).toHaveLength(1);
    expect(settled.messages[0]?.parts[0]).toMatchObject({ type: "text", text: "历史提问" });
  });

  it("重置对话时清空切换事务标记与缓存", () => {
    const held = conversationReducer(initialConversationState, conversationActions.sessionSwitchDeferred());
    const reset = conversationReducer(held, conversationActions.conversationReset());

    expect(reset.switchDeferred).toBe(false);
    expect(reset.deferredMessages).toBeNull();
    expect(reset.messages).toEqual([]);
  });
});