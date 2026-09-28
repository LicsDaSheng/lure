import { describe, expect, it } from "vitest";

import { initialPiSessionState, piSessionReducer } from "./conversation-domain";
import {
  messageText,
  messageThinking,
  type ConversationMessage,
  type EventEnvelope,
} from "@/lib/pi-rpc/types";

function reduce(events: EventEnvelope[]) {
  return events.reduce(piSessionReducer, initialPiSessionState);
}

function toolParts(message: ConversationMessage) {
  return message.parts.flatMap((part) => (part.type === "tool" ? [part] : []));
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
    expect(messageText(state.messages[0]!)).toBe("你好");
  });

  it("对初始 prompt 的 accepted 与 message_end 去重，但保留续跑用户消息", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      {
        sequence: 2,
        event: { type: "user_message_observed", message: "检查项目" },
      },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "第一步",
          thinking: "",
        },
      },
      {
        sequence: 5,
        event: { type: "user_message_observed", message: "继续检查" },
      },
    ]);

    expect(
      state.messages.filter((message) => message.role === "user"),
    ).toHaveLength(2);
    expect(state.messages.map(messageText)).toEqual([
      "检查项目",
      "第一步",
      "继续检查",
    ]);
  });

  it("进程退出会终结当前响应组，不再保留运行中状态", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "长任务",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_text_delta",
          contentIndex: 0,
          delta: "已完成一部分",
        },
      },
      { sequence: 5, event: { type: "process_exited", code: 1 } },
    ]);

    expect(state.connection.phase).toBe("failed");
    expect(state.activeAssistantId).toBeNull();
    expect(state.messages[1]).toMatchObject({
      turnPhase: "error",
      stopReason: "error",
      errorMessage: "Pi 进程已退出（退出码 1）",
    });
  });

  it("agent_settled 会分别定案同一运行中的多个 follow-up 响应组", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "第一个问题",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "第一个回答",
          thinking: "",
          stopReason: "stop",
        },
      },
      {
        sequence: 5,
        event: { type: "user_message_observed", message: "追加问题" },
      },
      { sequence: 6, event: { type: "turn_started" } },
      { sequence: 7, event: { type: "assistant_message_started" } },
      {
        sequence: 8,
        event: {
          type: "assistant_message_completed",
          text: "追加回答",
          thinking: "",
          stopReason: "aborted",
        },
      },
      { sequence: 9, event: { type: "run_settled" } },
    ]);

    const assistants = state.messages.filter(
      (message) => message.role === "assistant",
    );
    expect(assistants.map((message) => message.turnPhase)).toEqual([
      "settled",
      "aborted",
    ]);
    expect(assistants.map((message) => message.turnId)).toEqual([
      "user-1",
      "user-observed-5",
    ]);
  });

  it("按真实顺序交错保存文本、思考与工具调用", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "assistant_text_delta",
          contentIndex: 0,
          delta: "先读取配置",
        },
      },
      {
        sequence: 3,
        event: {
          type: "tool_started",
          toolCallId: "tool-1",
          toolName: "read",
          input: '{"path":"a"}',
        },
      },
      {
        sequence: 4,
        event: {
          type: "tool_completed",
          toolCallId: "tool-1",
          toolName: "read",
          output: "内容 A",
          isError: false,
        },
      },
      {
        sequence: 5,
        event: {
          type: "assistant_text_delta",
          contentIndex: 2,
          delta: "再看入口",
        },
      },
      {
        sequence: 6,
        event: {
          type: "tool_started",
          toolCallId: "tool-2",
          toolName: "read",
          input: '{"path":"b"}',
        },
      },
      {
        sequence: 7,
        event: {
          type: "assistant_thinking_delta",
          contentIndex: 4,
          delta: "整理结论",
        },
      },
    ]);

    const parts = state.messages[0]?.parts ?? [];
    expect(parts.map((part) => part.type)).toEqual([
      "text",
      "tool",
      "text",
      "tool",
      "thinking",
    ]);
    // 工具占据它在 assistant 内容中自己的位置，而不是堆到消息末尾。
    expect(parts.map((part) => part.contentIndex)).toEqual([0, 1, 2, 3, 4]);
    expect(parts[0]).toMatchObject({ type: "text", text: "先读取配置" });
    expect(parts[1]).toMatchObject({
      type: "tool",
      toolCallId: "tool-1",
      name: "read",
      status: "completed",
      output: "内容 A",
    });
    expect(parts[2]).toMatchObject({ type: "text", text: "再看入口" });
    expect(parts[3]).toMatchObject({
      type: "tool",
      toolCallId: "tool-2",
      status: "running",
    });
    expect(parts[4]).toMatchObject({ type: "thinking", text: "整理结论" });
  });

  it("message_end 的权威内容校正文本，同时保留已执行工具的位置", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: { type: "assistant_text_delta", contentIndex: 0, delta: "临时" },
      },
      {
        sequence: 3,
        event: {
          type: "tool_started",
          toolCallId: "tool-1",
          toolName: "read",
          input: "{}",
        },
      },
      {
        sequence: 4,
        event: {
          type: "assistant_text_delta",
          contentIndex: 2,
          delta: "临时结尾",
        },
      },
      {
        sequence: 5,
        event: {
          type: "assistant_message_completed",
          text: "正式开头正式结尾",
          thinking: "",
          blocks: [
            { contentIndex: 0, kind: "text", text: "正式开头" },
            { contentIndex: 2, kind: "text", text: "正式结尾" },
          ],
          stopReason: "stop",
        },
      },
    ]);

    const parts = state.messages[0]?.parts ?? [];
    expect(parts.map((part) => part.type)).toEqual(["text", "tool", "text"]);
    expect(parts[0]).toMatchObject({ contentIndex: 0, text: "正式开头" });
    expect(parts[2]).toMatchObject({ contentIndex: 2, text: "正式结尾" });
    expect(messageText(state.messages[0]!)).toBe("正式开头正式结尾");
  });

  it("将文本和思考增量归并到当前助手消息并以最终消息校正", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "assistant_thinking_delta",
          contentIndex: 0,
          delta: "分析",
        },
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

    expect(state.messages[0]?.role).toBe("assistant");
    expect(messageText(state.messages[0]!)).toBe("最终结果");
    expect(messageThinking(state.messages[0]!)).toBe("完整分析");
  });

  it("运行生命周期只更新独立运行状态，不写入对话消息", () => {
    const running = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "run_finished", willRetry: false } },
    ]);
    expect(running.connection.phase).toBe("running");
    expect(running.run.phase).toBe("running");
    expect(running.run.turnId).toBe("user-1");
    expect(running.messages).toHaveLength(1);
    expect(messageText(running.messages[0]!)).toBe("检查项目");

    const settled = piSessionReducer(running, {
      sequence: 4,
      event: { type: "run_settled" },
    });
    expect(settled.connection.phase).toBe("ready");
    expect(settled.run.phase).toBe("idle");
    expect(settled.messages).toEqual(running.messages);
  });

  it("agent_end 保留 willRetry 但不提前定案响应组", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "第一次尝试",
          thinking: "",
          stopReason: "stop",
        },
      },
      { sequence: 5, event: { type: "run_finished", willRetry: true } },
    ]);

    // 自动重试可能继续，这里不能把响应组当作已定案，也不能丢掉已有回复。
    expect(state.run.willRetry).toBe(true);
    expect(state.messages[1]).toMatchObject({
      turnId: "user-1",
      turnPhase: "running",
    });
    expect(state.messages[1]?.runDurationMs).toBeUndefined();
  });

  it("turn_end 提供单次轮次的权威终态，且不定案整次运行", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "长任务",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      { sequence: 4, event: { type: "turn_started" } },
      {
        sequence: 5,
        event: {
          type: "assistant_text_delta",
          contentIndex: 0,
          delta: "写了一半",
        },
      },
      { sequence: 6, event: { type: "turn_ended", stopReason: "aborted" } },
    ]);

    expect(state.messages[1]?.stopReason).toBe("aborted");
    expect(state.messages[1]?.turnPhase).toBe("running");
    expect(state.connection.phase).toBe("running");
  });

  it("turn_end 携带 pending 时不覆盖 message_end 已给出的真实终态", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "assistant_message_completed",
          text: "完整回答",
          thinking: "",
          stopReason: "stop",
        },
      },
      { sequence: 3, event: { type: "turn_ended", stopReason: "pending" } },
    ]);

    expect(state.messages[0]?.stopReason).toBe("stop");
  });

  it("run_settled 按 Pi 记录的终止原因定案响应组并记录耗时", () => {
    const settled = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
        receivedAtMs: 1_000,
      },
      { sequence: 2, event: { type: "run_started" }, receivedAtMs: 1_000 },
      {
        sequence: 3,
        event: { type: "assistant_message_started" },
        receivedAtMs: 1_200,
      },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "已完成",
          thinking: "",
          stopReason: "stop",
        },
        receivedAtMs: 50_000,
      },
      {
        sequence: 5,
        event: { type: "run_finished", willRetry: false },
        receivedAtMs: 50_100,
      },
      { sequence: 6, event: { type: "run_settled" }, receivedAtMs: 51_000 },
    ]);

    expect(settled.messages[1]).toMatchObject({
      turnId: "user-1",
      turnPhase: "settled",
      runStartedAtMs: 1_000,
      runSettledAtMs: 51_000,
      runDurationMs: 50_000,
    });

    const aborted = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "一半",
          thinking: "",
          stopReason: "aborted",
        },
      },
      { sequence: 5, event: { type: "run_settled" } },
    ]);
    expect(aborted.messages[1]?.turnPhase).toBe("aborted");

    const truncated = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "一半",
          thinking: "",
          stopReason: "length",
        },
      },
      { sequence: 5, event: { type: "run_settled" } },
    ]);
    expect(truncated.messages[1]?.turnPhase).toBe("truncated");

    const failed = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" } },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "一半",
          thinking: "",
          stopReason: "error",
          errorMessage: "模型连接中断",
        },
      },
      { sequence: 5, event: { type: "run_settled" } },
    ]);
    expect(failed.messages[1]?.turnPhase).toBe("error");
  });

  it("重试产生的失败响应保留在响应组内，不被后来者覆盖", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "检查项目",
        },
      },
      { sequence: 2, event: { type: "run_started" }, receivedAtMs: 0 },
      { sequence: 3, event: { type: "assistant_message_started" } },
      {
        sequence: 4,
        event: {
          type: "assistant_message_completed",
          text: "失败响应",
          thinking: "",
          stopReason: "error",
          errorMessage: "超时",
        },
      },
      { sequence: 5, event: { type: "run_finished", willRetry: true } },
      { sequence: 6, event: { type: "assistant_message_started" } },
      {
        sequence: 7,
        event: {
          type: "assistant_message_completed",
          text: "成功响应",
          thinking: "",
          stopReason: "stop",
        },
      },
      { sequence: 8, event: { type: "run_finished", willRetry: false } },
      { sequence: 9, event: { type: "run_settled" }, receivedAtMs: 9_000 },
    ]);

    expect(state.messages.map(messageText)).toEqual([
      "检查项目",
      "失败响应",
      "成功响应",
    ]);
    expect(state.messages[1]?.errorMessage).toBe("超时");
    // 自动重试计入用户感知总耗时：起点是首次 RunStarted。
    expect(state.messages[2]?.runStartedAtMs).toBe(0);
    expect(state.messages[2]?.runDurationMs).toBe(9_000);
    expect(
      state.messages
        .filter((message) => message.role === "assistant")
        .every((message) => message.turnPhase === "settled"),
    ).toBe(true);
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
        event: {
          type: "assistant_text_delta",
          contentIndex: 0,
          delta: "读取完成",
        },
      },
    ]);

    expect(state.messages).toHaveLength(2);
    expect(messageText(state.messages[0]!)).toBe("先读取文件");
    expect(toolParts(state.messages[0]!)[0]?.name).toBe("read");
    expect(messageText(state.messages[1]!)).toBe("读取完成");
  });

  it("按 contentIndex 保留多个思考块与正文的顺序", () => {
    const state = reduce([
      { sequence: 1, event: { type: "assistant_message_started" } },
      {
        sequence: 2,
        event: {
          type: "assistant_thinking_delta",
          contentIndex: 0,
          delta: "一",
        },
      },
      {
        sequence: 3,
        event: { type: "assistant_text_delta", contentIndex: 1, delta: "答案" },
      },
      {
        sequence: 4,
        event: {
          type: "assistant_thinking_delta",
          contentIndex: 2,
          delta: "二",
        },
      },
    ]);

    expect(state.messages[0]?.parts).toEqual([
      { id: "thinking-0", type: "thinking", contentIndex: 0, text: "一" },
      { id: "text-1", type: "text", contentIndex: 1, text: "答案" },
      { id: "thinking-2", type: "thinking", contentIndex: 2, text: "二" },
    ]);
    expect(messageText(state.messages[0]!)).toBe("答案");
    expect(messageThinking(state.messages[0]!)).toBe("一二");
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
          input: '{\n  "path": "README.md"\n}',
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

    expect(toolParts(state.messages[0]!)[0]).toMatchObject({
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
        event: {
          type: "compaction_changed",
          active: true,
          reason: "threshold",
        },
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
      event: {
        type: "extension_ui_resolved",
        requestId: "ui-1",
        cancelled: false,
      },
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
    expect(retrying.run.retry).toMatchObject({
      active: true,
      attempt: 2,
      maxAttempts: 3,
    });
    expect(retrying.messages).toEqual([]);

    const failed = reduce([
      { sequence: 1, event: { type: "process_exited", code: 1 } },
    ]);
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
      {
        sequence: 1,
        event: { type: "session_ready", snapshot: firstSnapshot },
      },
      {
        sequence: 2,
        event: {
          type: "user_message_accepted",
          requestId: "1",
          message: "旧对话",
        },
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
        snapshot: {
          ...firstSnapshot,
          sessionId: "session-2",
          sessionFile: "/tmp/2.jsonl",
        },
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

    expect(toolParts(state.messages[0]!)[0]?.status).toBe("completed");
    expect(state.connection.phase).toBe("failed");
    expect(state.error?.code).toBe("PROCESS_EXITED");
  });
});

describe("队列投影", () => {
  it("queue_changed 替换待处理队列", () => {
    const state = reduce([
      {
        sequence: 1,
        event: {
          type: "queue_changed",
          steering: ["先停下重构"],
          followUp: [],
        },
      },
      {
        sequence: 2,
        event: {
          type: "queue_changed",
          steering: ["先停下重构"],
          followUp: ["接着补测试"],
        },
      },
    ]);

    expect(state.queue).toEqual({
      steering: ["先停下重构"],
      followUp: ["接着补测试"],
    });
  });

  it("断连时清空队列", () => {
    const queued = reduce([
      {
        sequence: 1,
        event: {
          type: "queue_changed",
          steering: [],
          followUp: ["接着补测试"],
        },
      },
    ]);

    const disconnected = piSessionReducer(queued, {
      sequence: 2,
      event: {
        type: "connection_changed",
        snapshot: {
          phase: "disconnected",
          workingDirectory: null,
          sessionId: null,
          sessionFile: null,
          model: null,
          thinkingLevel: null,
          error: null,
        },
      },
    });

    expect(disconnected.queue).toEqual({ steering: [], followUp: [] });
  });

  it("切换到其他会话时清空队列", () => {
    const base = {
      phase: "ready" as const,
      workingDirectory: "/tmp",
      sessionFile: "/tmp/1.jsonl",
      model: null,
      thinkingLevel: null,
      error: null,
    };
    const queued = reduce([
      {
        sequence: 1,
        event: {
          type: "session_ready",
          snapshot: { ...base, sessionId: "session-1" },
        },
      },
      {
        sequence: 2,
        event: { type: "queue_changed", steering: ["插队"], followUp: [] },
      },
    ]);

    const switched = piSessionReducer(queued, {
      sequence: 3,
      event: {
        type: "session_ready",
        snapshot: { ...base, sessionId: "session-2" },
      },
    });

    expect(switched.queue).toEqual({ steering: [], followUp: [] });
  });
});
