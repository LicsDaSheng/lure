import { describe, expect, it } from "vitest";

import { conversationTurns } from "./conversation-selectors";
import type { ConversationMessage, MessagePart } from "@/lib/pi-rpc/types";

describe("conversationTurns", () => {
  function userMessage(id: string, text: string): ConversationMessage {
    return {
      id,
      role: "user",
      parts: [{ id: "text-0", type: "text", contentIndex: 0, text }],
    };
  }

  function assistantMessage(
    id: string,
    parts: MessagePart[],
    extra: Partial<ConversationMessage> = {},
  ): ConversationMessage {
    return { id, role: "assistant", parts, ...extra };
  }

  function textPart(text: string, contentIndex = 0): MessagePart {
    return { id: `text-${contentIndex}`, type: "text", contentIndex, text };
  }

  function toolPart(toolCallId: string, contentIndex = 1): MessagePart {
    return {
      id: toolCallId,
      type: "tool",
      contentIndex,
      toolCallId,
      name: "read",
      status: "completed",
      input: "{}",
      output: "输出",
      truncatedLines: null,
    };
  }

  it("单次纯文本回答不产生执行过程，直接作为最终结果", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "你好"),
        assistantMessage("assistant-1", [textPart("你好，有什么可以帮你")], {
          stopReason: "stop",
          turnId: "user-1",
          turnPhase: "settled",
        }),
      ],
      "ready",
      null,
    );

    expect(turns).toHaveLength(1);
    expect(turns[0]?.process).toEqual([]);
    expect(turns[0]?.result).toMatchObject({
      kind: "final",
      id: "assistant-1",
    });
    expect(turns[0]?.phase).toBe("settled");
  });

  it("运行中的最后一条纯文本消息是候选结果", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "检查项目"),
        assistantMessage("assistant-1", [textPart("正在读取")], {
          turnId: "user-1",
          turnPhase: "running",
        }),
      ],
      "running",
      "assistant-1",
    );

    expect(turns[0]?.isRunning).toBe(true);
    expect(turns[0]?.result).toMatchObject({ kind: "candidate" });
  });

  it("最后一条消息进入工具调用后，更早的纯文本不再充当结果", () => {
    const messages = [
      userMessage("user-1", "检查项目"),
      assistantMessage("assistant-1", [textPart("第一步完成")], {
        stopReason: "stop",
        turnId: "user-1",
        turnPhase: "running",
      }),
      assistantMessage(
        "assistant-2",
        [textPart("继续读取"), toolPart("tool-1")],
        {
          stopReason: "toolUse",
          turnId: "user-1",
          turnPhase: "running",
        },
      ),
    ];

    const running = conversationTurns(messages, "running", "assistant-2");
    expect(running[0]?.result).toBeNull();
    expect(running[0]?.process.map((group) => group.id)).toEqual([
      "assistant-1",
      "assistant-2",
    ]);

    const settled = conversationTurns(
      messages.map((message) =>
        message.role === "assistant"
          ? { ...message, turnPhase: "settled" as const }
          : message,
      ),
      "ready",
      null,
    );
    expect(settled[0]?.result).toBeNull();
  });

  it("带工具调用的消息整体进入执行过程，最后一轮纯文本才是结果", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "检查项目"),
        assistantMessage(
          "assistant-1",
          [textPart("先读取配置"), toolPart("tool-1")],
          {
            stopReason: "toolUse",
            turnId: "user-1",
            turnPhase: "settled",
          },
        ),
        assistantMessage("assistant-2", [textPart("配置已确认")], {
          stopReason: "stop",
          turnId: "user-1",
          turnPhase: "settled",
        }),
      ],
      "ready",
      null,
    );

    expect(turns[0]?.result).toMatchObject({
      kind: "final",
      id: "assistant-2",
    });
    expect(turns[0]?.toolCount).toBe(1);
    expect(turns[0]?.process.map((group) => group.id)).toEqual(["assistant-1"]);
    expect(turns[0]?.process[0]?.parts.map((part) => part.type)).toEqual([
      "text",
      "tool",
    ]);
  });

  it("自动重试时失败响应进入过程，最后一次成功响应才是结果", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "检查项目"),
        assistantMessage("assistant-1", [textPart("失败响应")], {
          stopReason: "error",
          errorMessage: "超时",
          turnId: "user-1",
          turnPhase: "settled",
        }),
        assistantMessage("assistant-2", [textPart("成功响应")], {
          stopReason: "stop",
          turnId: "user-1",
          turnPhase: "settled",
        }),
      ],
      "ready",
      null,
    );

    expect(turns[0]?.result).toMatchObject({
      kind: "final",
      id: "assistant-2",
    });
    expect(turns[0]?.process.map((group) => group.id)).toEqual(["assistant-1"]);
    expect(turns[0]?.process[0]?.errorMessage).toBe("超时");
  });

  it.each([
    ["aborted", "aborted"],
    ["length", "truncated"],
    ["error", "error"],
  ] as const)("%s 终止的响应只作为部分结果", (stopReason, turnPhase) => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "检查项目"),
        assistantMessage("assistant-1", [textPart("写了一半")], {
          stopReason,
          turnId: "user-1",
          turnPhase,
        }),
      ],
      "ready",
      null,
    );

    expect(turns[0]?.result).toMatchObject({ kind: "partial", stopReason });
    expect(turns[0]?.phase).toBe(turnPhase);
  });

  it("没有文本内容时只说明停止状态，不伪造结果内容", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "长任务"),
        assistantMessage("assistant-1", [], {
          stopReason: "aborted",
          turnId: "user-1",
          turnPhase: "aborted",
        }),
      ],
      "ready",
      null,
    );

    expect(turns[0]?.result).toMatchObject({ kind: "partial", parts: [] });
  });

  it("按用户指令划分多个响应组，每组各自定案", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "第一句"),
        assistantMessage("assistant-1", [textPart("第一答")], {
          stopReason: "stop",
          turnId: "user-1",
          turnPhase: "settled",
        }),
        userMessage("user-2", "第二句"),
        assistantMessage("assistant-2", [textPart("第二答")], {
          stopReason: "stop",
          turnId: "user-2",
          turnPhase: "settled",
        }),
      ],
      "ready",
      null,
    );

    expect(turns.map((turn) => turn.user?.id)).toEqual(["user-1", "user-2"]);
    expect(turns.map((turn) => turn.result?.id)).toEqual([
      "assistant-1",
      "assistant-2",
    ]);
  });

  it("历史会话沿用同一分类规则与耗时缺失", () => {
    const turns = conversationTurns(
      [
        userMessage("user-1", "检查项目"),
        assistantMessage(
          "assistant-1",
          [textPart("先读取配置"), toolPart("tool-1")],
          {
            stopReason: "toolUse",
            turnId: "user-1",
            turnPhase: "settled",
          },
        ),
        assistantMessage("assistant-2", [textPart("配置已确认")], {
          stopReason: "stop",
          turnId: "user-1",
          turnPhase: "settled",
        }),
      ],
      "ready",
      null,
    );

    expect(turns[0]?.durationMs).toBeNull();
    expect(turns[0]?.result?.kind).toBe("final");
    expect(turns[0]?.process).toHaveLength(1);
  });
});
