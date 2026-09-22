import { describe, expect, it } from "vitest";

import { conversationFromEntries } from "./conversation-domain";
import { messageText, type ConversationMessage, type SessionEntry, type ToolPart } from "@/lib/pi-rpc/types";

function messageEntry(id: string, parentId: string | null, message: SessionEntry["message"]): SessionEntry {
  return { type: "message", id, parentId, timestamp: "2026-09-20T10:00:00.000Z", message };
}

function toolParts(message: ConversationMessage): ToolPart[] {
  return message.parts.flatMap((part) => (part.type === "tool" ? [part] : []));
}

describe("conversationFromEntries", () => {
  it("按活动分支重建历史对话，并把工具结果回填到对应调用", () => {
    const entries: SessionEntry[] = [
      messageEntry("e1", null, { role: "user", content: "看一下配置文件" }),
      messageEntry("e2", "e1", {
        role: "assistant",
        content: [
          { type: "text", text: "先读取配置" },
          { type: "toolCall", id: "tool-1", name: "read", arguments: { path: "lure.toml" } },
          { type: "text", text: "然后汇报" },
        ],
        stopReason: "stop",
      }),
      messageEntry("e3", "e2", {
        role: "toolResult",
        toolCallId: "tool-1",
        toolName: "read",
        isError: false,
        content: [{ type: "text", text: "name = \"lure\"" }],
      }),
    ];

    const messages = conversationFromEntries(entries, "e3");

    expect(messages.map((message) => message.role)).toEqual(["user", "assistant"]);
    expect(messageText(messages[0]!)).toBe("看一下配置文件");
    expect(messageText(messages[1]!)).toBe("先读取配置然后汇报");

    const tools = toolParts(messages[1]!);
    expect(tools).toHaveLength(1);
    expect(tools[0]).toMatchObject({
      toolCallId: "tool-1",
      name: "read",
      status: "completed",
      output: "name = \"lure\"",
    });
    expect(tools[0]!.input).toContain("\"path\": \"lure.toml\"");
    // 工具在内容序列中的位置与 Pi 记录一致，位于两段文本之间。
    expect(messages[1]!.parts.map((part) => part.type)).toEqual(["text", "tool", "text"]);
  });

  it("只保留活动分支，忽略被放弃的历史分支", () => {
    const entries: SessionEntry[] = [
      messageEntry("e1", null, { role: "user", content: "起点" }),
      messageEntry("e2", "e1", { role: "assistant", content: [{ type: "text", text: "被放弃的回复" }] }),
      messageEntry("e3", "e1", { role: "assistant", content: [{ type: "text", text: "保留的回复" }] }),
    ];

    const messages = conversationFromEntries(entries, "e3");

    expect(messages.map(messageText)).toEqual(["起点", "保留的回复"]);
  });

  it("把失败的工具结果标记为错误状态", () => {
    const entries: SessionEntry[] = [
      messageEntry("e1", null, { role: "user", content: "运行测试" }),
      messageEntry("e2", "e1", {
        role: "assistant",
        content: [{ type: "toolCall", id: "tool-1", name: "bash", arguments: { command: "pnpm test" } }],
      }),
      messageEntry("e3", "e2", {
        role: "toolResult",
        toolCallId: "tool-1",
        toolName: "bash",
        isError: true,
        content: [{ type: "text", text: "1 test failed" }],
      }),
    ];

    const tools = toolParts(conversationFromEntries(entries, "e3")[1]!);

    expect(tools[0]).toMatchObject({ status: "error", output: "1 test failed" });
  });

  it("缺少叶子信息时按追加顺序展示消息", () => {
    const entries: SessionEntry[] = [
      messageEntry("e1", null, { role: "user", content: "第一句" }),
      messageEntry("e2", "e1", { role: "assistant", content: [{ type: "text", text: "回应" }] }),
    ];

    expect(conversationFromEntries(entries, null).map(messageText)).toEqual(["第一句", "回应"]);
  });

  it("跳过非消息条目并在空会话中返回空数组", () => {
    const entries: SessionEntry[] = [
      { type: "session_info", id: "i1", name: "任务名" },
      { type: "thinking_level_change", id: "t1", parentId: "i1" },
      messageEntry("e1", "t1", { role: "user", content: "只有一条" }),
    ];

    expect(conversationFromEntries(entries, "e1").map(messageText)).toEqual(["只有一条"]);
    expect(conversationFromEntries([], null)).toEqual([]);
  });
});
