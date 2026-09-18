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
