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

  it("运行生命周期只更新独立运行状态，不写入对话消息", () => {
    const running = reduce([
      {
        sequence: 1,
        event: { type: "user_message_accepted", requestId: "1", message: "检查项目" },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "run_finished", willRetry: false } },
    ]);
    expect(running.connection.phase).toBe("running");
    expect(running.run.phase).toBe("running");
    expect(running.messages).toHaveLength(1);
    expect(running.messages[0]?.content).toBe("检查项目");

    const settled = piSessionReducer(running, {
      sequence: 4,
      event: { type: "run_settled" },
    });
    expect(settled.connection.phase).toBe("ready");
    expect(settled.run.phase).toBe("idle");
    expect(settled.messages).toEqual(running.messages);
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

  it("将上下文压缩保存在独立运行状态中，不写入对话消息", () => {
    const compacting = reduce([
      {
        sequence: 1,
        event: { type: "compaction_changed", active: true, reason: "threshold" },
      },
    ]);
    expect(compacting.run.phase).toBe("compacting");
    expect(compacting.messages).toEqual([]);

    const settled = piSessionReducer(compacting, {
      sequence: 2,
      event: {
        type: "compaction_changed",
        active: false,
        reason: "threshold",
        aborted: false,
        summary: "摘要内容",
        tokensBefore: 32000,
      },
    });
    expect(settled.run.phase).toBe("idle");
    expect(settled.run.compaction).toMatchObject({
      active: false,
      summary: "摘要内容",
      tokensBefore: 32000,
    });
    expect(settled.messages).toEqual([]);
  });

  it("将扩展交互呈现为等待输入，并在响应后恢复执行", () => {
    const waiting = reduce([
      { sequence: 1, event: { type: "run_started" } },
      {
        sequence: 2,
        event: {
          type: "extension_ui_requested",
          requestId: "ui-1",
          method: "select",
          title: "选择范围",
          message: "请选择范围",
          options: ["当前文件", "整个项目"],
          placeholder: null,
          defaultValue: null,
        },
      },
    ]);

    expect(waiting.run.phase).toBe("waiting_input");
    expect(waiting.extensionRequest?.requestId).toBe("ui-1");
    expect(waiting.messages).toEqual([]);

    const resumed = piSessionReducer(waiting, {
      sequence: 3,
      event: { type: "extension_ui_resolved", requestId: "ui-1", cancelled: false },
    });
    expect(resumed.run.phase).toBe("running");
    expect(resumed.extensionRequest).toBeNull();
    expect(resumed.messages).toEqual([]);
  });

  it("停止、重试和连接失败都不生成对话消息", () => {
    const stopped = reduce([
      { sequence: 1, event: { type: "run_started" } },
      { sequence: 2, event: { type: "assistant_message_started" } },
      {
        sequence: 3,
        event: {
          type: "assistant_message_completed",
          text: "",
          thinking: "",
          stopReason: "aborted",
        },
      },
      { sequence: 4, event: { type: "run_settled" } },
    ]);
    expect(stopped.run.phase).toBe("idle");
    expect(stopped.messages).toHaveLength(1);
    expect(stopped.messages[0]?.stopReason).toBe("aborted");

    const retrying = reduce([
      { sequence: 1, event: { type: "run_started" } },
      {
        sequence: 2,
        event: {
          type: "retry_changed",
          active: true,
          attempt: 2,
          maxAttempts: 3,
          delayMs: 1000,
        },
      },
    ]);
    expect(retrying.run.phase).toBe("retrying");
    expect(retrying.run.retry).toMatchObject({ active: true, attempt: 2, maxAttempts: 3 });
    expect(retrying.messages).toEqual([]);

    const failed = reduce([{ sequence: 1, event: { type: "process_exited", code: 1 } }]);
    expect(failed.connection.phase).toBe("failed");
    expect(failed.run.phase).toBe("idle");
    expect(failed.messages).toEqual([]);
  });

  it("切换到新的 Pi session 时清空旧对话，重复就绪事件不清空当前对话", () => {
    const firstSnapshot = {
      phase: "ready" as const,
      workingDirectory: "/home/test/lure",
      sessionId: "session-1",
      sessionFile: "/tmp/1.jsonl",
      model: null,
      thinkingLevel: "medium",
      error: null,
    };
    const withMessage = reduce([
      { sequence: 1, event: { type: "session_ready", snapshot: firstSnapshot } },
      {
        sequence: 2,
        event: { type: "user_message_accepted", requestId: "1", message: "旧对话" },
      },
    ]);

    const duplicateReady = piSessionReducer(withMessage, {
      sequence: 3,
      event: { type: "session_ready", snapshot: firstSnapshot },
    });
    expect(duplicateReady.messages).toHaveLength(1);

    const nextSession = piSessionReducer(duplicateReady, {
      sequence: 4,
      event: {
        type: "session_ready",
        snapshot: { ...firstSnapshot, sessionId: "session-2", sessionFile: "/tmp/2.jsonl" },
      },
    });
    expect(nextSession.messages).toEqual([]);
    expect(nextSession.run.phase).toBe("idle");
    expect(nextSession.connection.sessionId).toBe("session-2");
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
