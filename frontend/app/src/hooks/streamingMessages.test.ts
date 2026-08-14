import { describe, expect, it } from "vitest";
import {
  appendAssistantContent,
  appendAssistantReasoning,
  appendAssistantTool,
  finalizeAssistant,
  type UiMessage,
} from "./streamingMessages";

const nextId = () => "assistant-1";

describe("流式 assistant 消息归并", () => {
  it("reasoning-first 消息在正文和终帧到达后仍启用打字机", () => {
    let messages: UiMessage[] = [
      { id: "user-1", role: "user", content: "你好" },
    ];

    messages = appendAssistantReasoning(messages, "思考", nextId);
    messages = appendAssistantContent(messages, "答", nextId);
    messages = finalizeAssistant(messages, "答案", nextId);

    expect(messages.at(-1)).toMatchObject({
      id: "assistant-1",
      role: "assistant",
      content: "答案",
      reasoning: "思考",
      streaming: false,
      typewriter: true,
    });
  });

  it("content-first 消息持续合并 delta 并保留打字机标记", () => {
    let messages: UiMessage[] = [];

    messages = appendAssistantContent(messages, "你", nextId);
    messages = appendAssistantContent(messages, "好", nextId);

    expect(messages).toEqual([
      {
        id: "assistant-1",
        role: "assistant",
        content: "你好",
        streaming: true,
        typewriter: true,
        reasoningDone: true,
      },
    ]);
  });

  it("连续 reasoning delta 合并到同一消息", () => {
    let messages: UiMessage[] = [];

    messages = appendAssistantReasoning(messages, "先", nextId);
    messages = appendAssistantReasoning(messages, "想", nextId);

    expect(messages.at(-1)).toMatchObject({
      content: "",
      reasoning: "先想",
      streaming: true,
      typewriter: true,
    });
  });

  it("content delta 到达后 reasoning 标记为完成", () => {
    let messages: UiMessage[] = [];
    messages = appendAssistantReasoning(messages, "思考", nextId);
    messages = appendAssistantContent(messages, "答", nextId);

    expect(messages.at(-1)).toMatchObject({
      reasoning: "思考",
      content: "答",
      streaming: true,
      reasoningDone: true,
    });
  });

  it("终帧到达后 reasoning 标记为完成", () => {
    let messages: UiMessage[] = [];
    messages = appendAssistantReasoning(messages, "思考", nextId);
    messages = finalizeAssistant(messages, "答案", nextId);

    expect(messages.at(-1)).toMatchObject({
      reasoning: "思考",
      content: "答案",
      streaming: false,
      reasoningDone: true,
    });
  });

  it("缺少权威全文时以已累积正文完成消息", () => {
    const streaming: UiMessage[] = [
      {
        id: "assistant-1",
        role: "assistant",
        content: "已有正文",
        streaming: true,
        typewriter: true,
      },
    ];

    expect(finalizeAssistant(streaming, undefined, nextId).at(-1)).toMatchObject(
      {
        content: "已有正文",
        streaming: false,
        typewriter: true,
      },
    );
  });

  it("没有流式占位时以终帧创建消息，空终帧则保持原列表", () => {
    const messages: UiMessage[] = [];
    const finalized = finalizeAssistant(messages, "完整回答", nextId);

    expect(finalized.at(-1)).toMatchObject({
      content: "完整回答",
      typewriter: true,
    });
    expect(finalizeAssistant(messages, undefined, nextId)).toBe(messages);
  });

  it("工具事件追加到当前 assistant，并在终帧到达后标记完成", () => {
    let messages: UiMessage[] = [
      { id: "user-1", role: "user", content: "读取文件" },
    ];

    messages = appendAssistantTool(messages, "read_file", nextId);
    expect(messages.at(-1)).toMatchObject({
      role: "assistant",
      content: "",
      streaming: true,
      tools: [
        { id: "assistant-1-tool-1", name: "read_file", status: "running" },
      ],
    });

    messages = finalizeAssistant(messages, "读取完成", nextId);
    expect(messages.at(-1)).toMatchObject({
      content: "读取完成",
      tools: [
        { id: "assistant-1-tool-1", name: "read_file", status: "complete" },
      ],
    });
  });

  it("历史工具可以直接以完成态附着到 assistant 消息", () => {
    const messages = appendAssistantTool([], "web_search", nextId, false);

    expect(messages.at(-1)?.tools).toEqual([
      { id: "assistant-1-tool-1", name: "web_search", status: "complete" },
    ]);
    expect(messages.at(-1)?.streaming).toBeUndefined();
  });
});
