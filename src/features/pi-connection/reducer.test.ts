import { describe, expect, it } from "vitest";

import {
  initialPiSessionState,
  piSessionReducer,
  type EventEnvelope,
} from "./reducer";

function reduce(events: EventEnvelope[]) {
  return events.reduce(piSessionReducer, initialPiSessionState);
}

describe("piSessionReducer", () => {
  it("只为同一个已接受请求添加一条用户消息", () => {
    const event: EventEnvelope = {
      sequence: 1,
      event: {
        type: "user_message_accepted",
        requestId: "request-1",
        message: "你好",
      },
    };

    const state = reduce([event, { ...event, sequence: 2 }]);

    expect(state.messages).toHaveLength(1);
    expect(state.messages[0]?.content).toBe("你好");
  });

  it("将文本和思考增量归并到当前助手消息并以最终消息校正", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: { type: "assistant_thinking_delta", contentIndex: 0, delta: "分析" },
      },
      {
        sequence: 3,
        event: { type: "assistant_text_delta", contentIndex: 1, delta: "临时" },
      },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "最终结果",
          thinking: "完整分析",
        },
      },
    ]);

    expect(state.messages[0]).toMatchObject({
      role: "assistant",
      content: "最终结果",
      thinking: "完整分析",
    });
  });

  it("仅在 run_settled 后恢复 ready", () => {
    const running = reduce([
      { sequence: 1, event: { type: "run_started" } },
      { sequence: 2, event: { type: "run_finished", willRetry: false } },
    ]);
    expect(running.connection.phase).toBe("running");

    const settled = piSessionReducer(running, {
      sequence: 3,
      event: { type: "run_settled" },
    });
    expect(settled.connection.phase).toBe("ready");
  });

  it("工具调用后的新助手轮次保留前一轮内容", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "assistant_message_completed",
          text: "先读取文件",
          thinking: "",
        },
      },
      {
        sequence: 3,
        event: { type: "tool_started", toolCallId: "tool-1", toolName: "read" },
      },
      { sequence: 4, event: { type: "assistant_message_started" } },
      {
        sequence: 5,
        event: { type: "assistant_text_delta", contentIndex: 0, delta: "读取完成" },
      },
    ]);

    expect(state.messages).toHaveLength(2);
    expect(state.messages[0]?.content).toBe("先读取文件");
    expect(state.messages[0]?.tools[0]?.name).toBe("read");
    expect(state.messages[1]?.content).toBe("读取完成");
  });

  it("按 contentIndex 保留多个思考块与正文的顺序", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      { sequence: 2, event: { type: "assistant_thinking_delta", contentIndex: 0, delta: "一" } },
      { sequence: 3, event: { type: "assistant_text_delta", contentIndex: 1, delta: "答案" } },
      { sequence: 4, event: { type: "assistant_thinking_delta", contentIndex: 2, delta: "二" } },
    ]);

    expect(state.messages[0]?.blocks).toEqual([
      { type: "thinking", contentIndex: 0, text: "一" },
      { type: "text", contentIndex: 1, text: "答案" },
      { type: "thinking", contentIndex: 2, text: "二" },
    ]);
  });

  it("保留工具参数、输出及截断行数", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "tool_started",
          toolCallId: "tool-1",
          toolName: "read",
          input: "{\n  \"path\": \"README.md\"\n}",
        },
      },
      {
        sequence: 3,
        event: {
          type: "tool_completed",
          toolCallId: "tool-1",
          toolName: "read",
          output: "line 1\nline 2",
          truncatedLines: 12,
          isError: false,
        },
      },
    ]);

    expect(state.messages[0]?.tools[0]).toMatchObject({
      input: expect.stringContaining("README.md"),
      output: "line 1\nline 2",
      truncatedLines: 12,
      status: "completed",
    });
  });

  it("将压缩摘要加入时间线并保留 token 数", () => {
    const state = reduce([
      {
        sequence: 1,
        event: { type: "compaction_changed", active: true, reason: "threshold" },
      },
      {
        sequence: 2,
        event: {
          type: "compaction_changed",
          active: false,
          reason: "threshold",
          aborted: false,
          summary: "摘要内容",
          tokensBefore: 32000,
        },
      },
    ]);

    expect(state.messages[0]).toMatchObject({
      role: "system",
      kind: "compaction",
      content: "摘要内容",
      tokensBefore: 32000,
      pending: false,
    });
  });

  it("跟踪工具状态和进程错误", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: { type: "tool_started", toolCallId: "tool-1", toolName: "read" },
      },
      {
        sequence: 3,
        event: {
          type: "tool_completed",
          toolCallId: "tool-1",
          toolName: "read",
          isError: false,
        },
      },
      { sequence: 4, event: { type: "process_exited", code: 1 } },
    ]);

    expect(state.messages[0]?.tools[0]?.status).toBe("completed");
    expect(state.connection.phase).toBe("failed");
    expect(state.error?.code).toBe("PROCESS_EXITED");
  });
});
