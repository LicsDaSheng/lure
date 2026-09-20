import { describe, expect, it } from "vitest";

import type { ConversationMessage } from "@/features/pi-connection/reducer";
import { convertPiMessage, readAppendMessageText } from "./assistant-runtime";

describe("assistant-ui 消息适配", () => {
  it("按 parts 的真实顺序转换文本、思考与工具调用", () => {
    const message: ConversationMessage = {
      id: "assistant-1",
      role: "assistant",
      parts: [
        { id: "thinking-0", type: "thinking", contentIndex: 0, text: "先读取配置" },
        { id: "text-1", type: "text", contentIndex: 1, text: "看配置：", },
        {
          id: "tool-1",
          type: "tool",
          contentIndex: 2,
          toolCallId: "tool-1",
          name: "read",
          status: "completed",
          input: "{\"path\":\"README.md\"}",
          output: "文件内容",
          truncatedLines: null,
        },
        { id: "text-3", type: "text", contentIndex: 3, text: "读取完成" },
      ],
    };

    expect(convertPiMessage(message, false)).toEqual({
      id: "assistant-1",
      role: "assistant",
      content: [
        { type: "reasoning", text: "先读取配置", status: { type: "complete" } },
        { type: "text", text: "看配置：", status: { type: "complete" } },
        {
          type: "tool-call",
          toolCallId: "tool-1",
          toolName: "read",
          args: { path: "README.md" },
          argsText: "{\"path\":\"README.md\"}",
          result: "文件内容",
          isError: false,
          artifact: { status: "completed", truncatedLines: null },
        },
        { type: "text", text: "读取完成", status: { type: "complete" } },
      ],
      status: { type: "complete", reason: "stop" },
    });
  });

  it("保留无效工具参数原文，并映射流式与错误状态", () => {
    const message: ConversationMessage = {
      id: "assistant-2",
      role: "assistant",
      parts: [
        {
          id: "tool-2",
          type: "tool",
          contentIndex: 0,
          toolCallId: "tool-2",
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
          artifact: { status: "error", truncatedLines: null },
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