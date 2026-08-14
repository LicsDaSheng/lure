// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { renderHook } from "@testing-library/react";
import { convertLureMessage, useLureRuntime, type LureMessage } from "./runtime.js";

describe("convertLureMessage 映射三段消息", () => {
  it("maps reasoning + text + tool-call parts", () => {
    const m: LureMessage = {
      id: "m1",
      role: "assistant",
      content: "answer",
      reasoningContent: "thinking",
      toolCalls: [{ id: "call_1", name: "echo", arguments: `{"text":"hi"}` }],
    };
    expect(convertLureMessage(m).content).toEqual([
      { type: "reasoning", text: "thinking" },
      { type: "text", text: "answer" },
      { type: "tool-call", toolCallId: "call_1", toolName: "echo", argsText: `{"text":"hi"}` },
    ]);
  });

  it("maps user message with only text", () => {
    const m: LureMessage = { id: "u1", role: "user", content: "hi" };
    expect(convertLureMessage(m).content).toEqual([{ type: "text", text: "hi" }]);
  });

  it("omits empty reasoning and tool calls", () => {
    const m: LureMessage = { id: "m2", role: "assistant", content: "x" };
    expect(convertLureMessage(m).content).toEqual([{ type: "text", text: "x" }]);
  });
});

describe("useExternalStoreRuntime 接受适配器", () => {
  it("returns a runtime", () => {
    const messages: LureMessage[] = [
      { id: "m1", role: "assistant", content: "hi", reasoningContent: "think" },
    ];
    const { result } = renderHook(() =>
      useLureRuntime({ messages, onNew: async () => {} }),
    );
    expect(result.current).toBeTruthy();
  });
});
