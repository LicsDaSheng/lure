import type {
  ConnectionSnapshot,
  ConversationMessage,
  EventEnvelope,
  MessagePart,
  PiSessionState,
  RunState,
  ToolPart,
} from "./pi-session-types";

export const disconnectedSnapshot: ConnectionSnapshot = {
  phase: "disconnected",
  workingDirectory: null,
  sessionId: null,
  sessionFile: null,
  model: null,
  thinkingLevel: null,
  error: null,
};

const idleRunState = (): RunState => ({
  phase: "idle",
  retry: null,
  compaction: null,
});

export const initialPiSessionState: PiSessionState = {
  connection: disconnectedSnapshot,
  messages: [],
  activeAssistantId: null,
  error: null,
  notice: null,
  diagnostics: [],
  run: idleRunState(),
  extensionRequest: null,
};

export function piSessionReducer(
  state: PiSessionState,
  envelope: EventEnvelope,
): PiSessionState {
  const event = envelope.event;
  switch (event.type) {
    case "connection_changed":
      return {
        ...state,
        connection: event.snapshot,
        error: event.snapshot.error,
        ...(event.snapshot.phase === "disconnected" || event.snapshot.phase === "failed"
          ? { extensionRequest: null, run: idleRunState() }
          : {}),
      };
    case "session_ready": {
      const sessionChanged = state.connection.sessionId !== event.snapshot.sessionId;
      return {
        ...state,
        connection: event.snapshot,
        error: event.snapshot.error,
        ...(sessionChanged
          ? {
              messages: [],
              activeAssistantId: null,
              notice: null,
              diagnostics: [],
              run: idleRunState(),
              extensionRequest: null,
            }
          : {}),
      };
    }
    case "user_message_accepted": {
      if (state.messages.some((message) => message.id === `user-${event.requestId}`)) {
        return state;
      }
      return {
        ...state,
        messages: [
          ...state.messages,
          {
            id: `user-${event.requestId}`,
            role: "user",
            parts: [{ id: "text-0", type: "text", contentIndex: 0, text: event.message }],
          },
        ],
      };
    }
    case "assistant_message_started":
      return startAssistantMessage(state, envelope.sequence);
    case "assistant_text_delta":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        parts: appendTextDelta(message.parts, "text", event.contentIndex, event.delta),
      }));
    case "assistant_thinking_delta":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        parts: appendTextDelta(message.parts, "thinking", event.contentIndex, event.delta),
      }));
    case "assistant_message_completed":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        parts:
          event.blocks && event.blocks.length > 0
            ? mergeCompletedParts(message.parts, event.blocks)
            : fallbackCompletedParts(message.parts, event.text, event.thinking),
        stopReason: event.stopReason,
        errorMessage: event.errorMessage,
      }));
    case "tool_started":
      return updateTool(state, envelope.sequence, event.toolCallId, event.toolName, "running", {
        input: event.input,
      });
    case "tool_updated":
      return updateTool(state, envelope.sequence, event.toolCallId, event.toolName, "running", event);
    case "tool_completed":
      return updateTool(
        state,
        envelope.sequence,
        event.toolCallId,
        event.toolName,
        event.isError ? "error" : "completed",
        event,
      );
    case "run_started":
      return {
        ...state,
        connection: { ...state.connection, phase: "running" },
        run: { ...state.run, phase: "running", retry: null },
      };
    case "run_finished":
      return state;
    case "run_settled":
      return {
        ...state,
        connection: { ...state.connection, phase: "ready" },
        activeAssistantId: null,
        run: idleRunState(),
      };
    case "retry_changed":
      return {
        ...state,
        notice: event.active ? event.message ?? null : null,
        run: {
          ...state.run,
          phase: event.active
            ? "retrying"
            : state.connection.phase === "running"
              ? "running"
              : "idle",
          retry: {
            active: event.active,
            attempt: event.attempt ?? null,
            maxAttempts: event.maxAttempts ?? null,
            delayMs: event.delayMs ?? null,
            message: event.message ?? null,
          },
        },
      };
    case "compaction_changed":
      return {
        ...state,
        run: {
          ...state.run,
          phase: event.active
            ? "compacting"
            : state.connection.phase === "running"
              ? "running"
              : "idle",
          compaction: {
            active: event.active,
            reason: event.reason ?? null,
            aborted: event.aborted ?? null,
            summary: event.summary ?? null,
            tokensBefore: event.tokensBefore ?? null,
            errorMessage: event.errorMessage ?? null,
          },
        },
      };
    case "extension_ui_requested":
      return {
        ...state,
        run: { ...state.run, phase: "waiting_input" },
        extensionRequest: {
          requestId: event.requestId,
          method: event.method,
          title: event.title,
          message: event.message,
          options: event.options,
          placeholder: event.placeholder,
          defaultValue: event.defaultValue,
        },
      };
    case "extension_ui_resolved":
      return {
        ...state,
        run: {
          ...state.run,
          phase: state.connection.phase === "running" ? "running" : "idle",
        },
        extensionRequest:
          state.extensionRequest?.requestId === event.requestId
            ? null
            : state.extensionRequest,
      };
    case "notification":
      return { ...state, notice: event.message };
    case "process_stderr":
      return {
        ...state,
        diagnostics: [...state.diagnostics, event.message].slice(-5),
      };
    case "process_exited": {
      const error = {
        code: "PROCESS_EXITED",
        message: `Pi 进程已退出${event.code == null ? "" : `（退出码 ${event.code}）`}`,
      };
      return {
        ...state,
        connection: { ...state.connection, phase: "failed", error },
        error,
        run: idleRunState(),
      };
    }
    case "protocol_error": {
      const error = { code: "RPC_PROTOCOL_ERROR", message: event.message };
      return {
        ...state,
        connection: { ...state.connection, phase: "failed", error },
        error,
        run: idleRunState(),
      };
    }
  }
}

function startAssistantMessage(state: PiSessionState, sequence: number): PiSessionState {
  const id = `assistant-${sequence}`;
  return {
    ...state,
    activeAssistantId: id,
    messages: [
      ...state.messages,
      {
        id,
        role: "assistant",
        parts: [],
      },
    ],
  };
}

function updateActiveAssistant(
  state: PiSessionState,
  sequence: number,
  update: (message: ConversationMessage) => ConversationMessage,
): PiSessionState {
  const started = state.activeAssistantId ? state : startAssistantMessage(state, sequence);
  const id = started.activeAssistantId;
  return {
    ...started,
    messages: started.messages.map((message) =>
      message.id === id ? update(message) : message,
    ),
  };
}

function sortParts(parts: MessagePart[]): MessagePart[] {
  return [...parts].sort((left, right) => left.contentIndex - right.contentIndex);
}

function nextContentIndex(parts: MessagePart[]): number {
  return parts.reduce((max, part) => Math.max(max, part.contentIndex), -1) + 1;
}

function appendTextDelta(
  parts: MessagePart[],
  type: "text" | "thinking",
  contentIndex: number,
  delta: string,
): MessagePart[] {
  const existing = parts.find(
    (part) => part.type === type && part.contentIndex === contentIndex,
  );
  if (existing && existing.type !== "tool") {
    return parts.map((part) =>
      part === existing ? { ...existing, text: existing.text + delta } : part,
    );
  }
  const appended: MessagePart = {
    id: `${type}-${contentIndex}`,
    type,
    contentIndex,
    text: delta,
  };
  // Pi 的 contentIndex 单调递增，事件到达顺序就是真实输出顺序；
  // 这里按到达顺序追加，只在 message_end 用权威内容重排。
  return [...parts, appended];
}

/** 用 message_end 的权威内容重建文本与思考，同时保留已执行工具的位置。 */
function mergeCompletedParts(
  parts: MessagePart[],
  blocks: Array<{ contentIndex: number; kind: "text" | "thinking"; text: string }>,
): MessagePart[] {
  const rebuilt: MessagePart[] = blocks.map((block) => ({
    id: `${block.kind}-${block.contentIndex}`,
    type: block.kind,
    contentIndex: block.contentIndex,
    text: block.text,
  }));
  const tools = parts.filter((part): part is ToolPart => part.type === "tool");
  return sortParts([...rebuilt, ...tools]);
}

/**
 * message_end 的兜底：Pi 正常会给出块级内容，缺失时用聚合文本重建，
 * 避免丢掉 Pi 已经返回的完整消息。
 */
function fallbackCompletedParts(
  parts: MessagePart[],
  text: string,
  thinking: string,
): MessagePart[] {
  const rebuilt: MessagePart[] = [];
  const base = parts.some((part) => part.type === "tool") ? nextContentIndex(parts) : 0;
  if (thinking) {
    rebuilt.push({ id: `thinking-${base}`, type: "thinking", contentIndex: base, text: thinking });
  }
  if (text) {
    const index = base + rebuilt.length;
    rebuilt.push({ id: `text-${index}`, type: "text", contentIndex: index, text });
  }
  if (rebuilt.length === 0) return parts;
  const tools = parts.filter((part): part is ToolPart => part.type === "tool");
  return sortParts([...rebuilt, ...tools]);
}

function upsertToolPart(
  parts: MessagePart[],
  toolCallId: string,
  name: string,
  status: ToolPart["status"],
  details: { input?: string; output?: string; truncatedLines?: number | null },
): MessagePart[] {
  const existing = parts.find(
    (part): part is ToolPart => part.type === "tool" && part.toolCallId === toolCallId,
  );
  if (existing) {
    return parts.map((part) =>
      part === existing
        ? {
            ...existing,
            name,
            status,
            input: details.input || existing.input,
            output: details.output ?? existing.output,
            truncatedLines: details.truncatedLines ?? existing.truncatedLines,
          }
        : part,
    );
  }
  // 工具在 assistant 内容里占据自己的位置：Pi 的 contentIndex 单调递增，
  // 所以新工具的位置就是当前已知最大索引的下一位。
  const tool: MessagePart = {
    id: toolCallId,
    type: "tool",
    contentIndex: nextContentIndex(parts),
    toolCallId,
    name,
    status,
    input: details.input ?? "",
    output: details.output ?? "",
    truncatedLines: details.truncatedLines ?? null,
  };
  return [...parts, tool];
}

function updateTool(
  state: PiSessionState,
  sequence: number,
  toolCallId: string,
  name: string,
  status: ToolPart["status"],
  details: { input?: string; output?: string; truncatedLines?: number | null },
): PiSessionState {
  return updateActiveAssistant(state, sequence, (message) => ({
    ...message,
    parts: upsertToolPart(message.parts, toolCallId, name, status, details),
  }));
}
