import { describe, expect, it } from "vitest";

import type { ConversationMessage } from "@/features/pi-connection/reducer";
import { convertPiMessage, readAppendMessageText } from "./assistant-runtime";

describe("assistant-ui Pi 消息适配", () => {
  it("按原始顺序转换文本、思考和工具调用", () => {
    const message: ConversationMessage = {
      id: "assistant-1",
      role: "assistant",
      content: "检查完成",
      thinking: "先读取配置",
      blocks: [
        { type: "thinking", contentIndex: 0, text: "先读取配置" },
        { type: "text", contentIndex: 1, text: "检查完成" },
      ],
      tools: [
        {
          id: "tool-1",
          name: "read",
          status: "completed",
          input: "{\"path\":\"README.md\"}",
          output: "文件内容",
          truncatedLines: null,
        },
      ],
    };

    expect(convertPiMessage(message, false)).toEqual({
      id: "assistant-1",
      role: "assistant",
      content: [
        {
          type: "reasoning",
          text: "先读取配置",
          status: { type: "complete" },
        },
        {
          type: "text",
          text: "检查完成",
          status: { type: "complete" },
        },
        {
          type: "tool-call",
          toolCallId: "tool-1",
          toolName: "read",
          args: { path: "README.md" },
          argsText: "{\"path\":\"README.md\"}",
          result: "文件内容",
          isError: false,
        },
      ],
      status: { type: "complete", reason: "stop" },
      metadata: { custom: { sourceMessageId: "assistant-1" } },
    });
  });

  it("保留无效工具参数原文，并映射流式与错误状态", () => {
    const message: ConversationMessage = {
      id: "assistant-2",
      role: "assistant",
      content: "",
      thinking: "",
      blocks: [],
      tools: [
        {
          id: "tool-2",
          name: "bash",
          status: "error",
          input: "not-json",
          output: "执行失败",
          truncatedLines: null,
        },
      ],
      errorMessage: "工具执行失败",
    };

    expect(convertPiMessage(message, true)).toMatchObject({
      content: [
        {
          type: "tool-call",
          args: {},
          argsText: "not-json",
          isError: true,
          result: "执行失败",
        },
      ],
      status: { type: "running" },
    });

    expect(convertPiMessage(message, false)).toMatchObject({
      status: { type: "incomplete", reason: "error", error: "工具执行失败" },
    });
  });

  it("只从 assistant-ui 新消息中提取文本内容", () => {
    expect(
      readAppendMessageText({
        role: "user",
        content: [
          { type: "text", text: "第一段" },
          { type: "text", text: "第二段" },
        ],
      }),
    ).toBe("第一段\n第二段");
    expect(readAppendMessageText({ role: "user", content: [] })).toBe("");
  });
});
