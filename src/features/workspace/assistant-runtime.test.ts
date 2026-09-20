import { describe, expect, it } from "vitest";

import type { ConversationMessage } from "@/features/pi-connection/reducer";
import {
  convertPiMessage,
  readAppendMessageImages,
  readAppendMessageText,
} from "./assistant-runtime";

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

    expect(convertPiMessage(message, false)).toMatchObject({
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
          artifact: {
            id: "tool-1",
            name: "read",
            status: "completed",
            input: "{\"path\":\"README.md\"}",
            output: "文件内容",
            truncatedLines: null,
          },
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
          artifact: {
            id: "tool-2",
            name: "bash",
            status: "error",
            input: "not-json",
            output: "执行失败",
            truncatedLines: null,
          },
        },
      ],
      status: { type: "running" },
    });

    expect(convertPiMessage(message, false)).toMatchObject({
      status: { type: "incomplete", reason: "error", error: "工具执行失败" },
    });
  });

  it("从 assistant-ui 新消息中提取文本与图片附件", () => {
    const message = {
      role: "user" as const,
      content: [
        { type: "text" as const, text: "第一段" },
        { type: "text" as const, text: "第二段" },
      ],
      attachments: [
        {
          id: "image-1",
          type: "image" as const,
          name: "shot.png",
          contentType: "image/png",
          status: { type: "complete" as const },
          content: [
            {
              type: "image" as const,
              image: "data:image/png;base64,aGVsbG8=",
              filename: "shot.png",
            },
          ],
        },
      ],
    };

    expect(readAppendMessageText(message)).toBe("第一段\n第二段");
    expect(readAppendMessageImages(message)).toEqual([
      { data: "aGVsbG8=", mimeType: "image/png" },
    ]);
    expect(readAppendMessageText({ role: "user", content: [] })).toBe("");
    expect(readAppendMessageImages({ role: "user", content: [] })).toEqual([]);
  });
});