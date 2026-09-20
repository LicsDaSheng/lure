export type ConnectionPhase =
  | "disconnected"
  | "connecting"
  | "ready"
  | "running"
  | "failed";

export type LureError = {
  code: string;
  message: string;
};

export type ModelSnapshot = { provider: string; id: string };

export type ConnectionSnapshot = {
  phase: ConnectionPhase;
  workingDirectory: string | null;
  sessionId: string | null;
  sessionFile: string | null;
  model: ModelSnapshot | null;
  thinkingLevel: string | null;
  error: LureError | null;
};

export type RunStatus =
  | "idle"
  | "running"
  | "waiting_input"
  | "completed"
  | "failed"
  | "stopped";

export type ExtensionUiRequest = {
  requestId: string;
  method: "select" | "confirm" | "input" | "editor" | string;
  title: string | null;
  message: string | null;
  options: string[];
  placeholder: string | null;
  defaultValue: string | null;
};

export type MessageBlock = {
  type: "text" | "thinking";
  contentIndex: number;
  text: string;
};

export type ToolRun = {
  id: string;
  name: string;
  status: "running" | "completed" | "error";
  input: string;
  output: string;
  truncatedLines: number | null;
};

export type ConversationMessage = {
  id: string;
  role: "assistant" | "user" | "system";
  kind: "message" | "compaction" | "branch_summary" | "status";
  content: string;
  thinking: string;
  blocks: MessageBlock[];
  tools: ToolRun[];
  stopReason?: string | null;
  errorMessage?: string | null;
  tokensBefore?: number | null;
  pending?: boolean;
  status?: RunStatus;
};

export type PiEvent =
  | { type: "connection_changed"; snapshot: ConnectionSnapshot }
  | { type: "session_ready"; snapshot: ConnectionSnapshot }
  | { type: "user_message_accepted"; requestId: string; message: string }
  | { type: "assistant_message_started" }
  | { type: "assistant_text_delta"; contentIndex: number; delta: string }
  | { type: "assistant_thinking_delta"; contentIndex: number; delta: string }
  | {
      type: "assistant_message_completed";
      text: string;
      thinking: string;
      blocks?: Array<{ contentIndex: number; kind: "text" | "thinking"; text: string }>;
      stopReason?: string | null;
      errorMessage?: string | null;
    }
  | { type: "tool_started"; toolCallId: string; toolName: string; input?: string }
  | {
      type: "tool_updated";
      toolCallId: string;
      toolName: string;
      input?: string;
      output?: string;
      truncatedLines?: number | null;
    }
  | {
      type: "tool_completed";
      toolCallId: string;
      toolName: string;
      input?: string;
      output?: string;
      truncatedLines?: number | null;
      isError: boolean;
    }
  | { type: "run_started" }
  | { type: "run_finished"; willRetry: boolean }
  | { type: "run_settled" }
  | {
      type: "retry_changed";
      active: boolean;
      attempt?: number | null;
      maxAttempts?: number | null;
      delayMs?: number | null;
      message?: string | null;
    }
  | {
      type: "compaction_changed";
      active: boolean;
      reason?: string | null;
      aborted?: boolean | null;
      summary?: string | null;
      tokensBefore?: number | null;
      errorMessage?: string | null;
    }
  | ({ type: "extension_ui_requested" } & ExtensionUiRequest)
  | { type: "extension_ui_resolved"; requestId: string; cancelled: boolean }
  | { type: "notification"; level: string; message: string }
  | { type: "process_stderr"; message: string }
  | { type: "process_exited"; code?: number | null }
  | { type: "protocol_error"; message: string };

export type EventEnvelope = {
  sequence: number;
  event: PiEvent;
};

export type PiSessionState = {
  connection: ConnectionSnapshot;
  messages: ConversationMessage[];
  activeAssistantId: string | null;
  error: LureError | null;
  notice: string | null;
  diagnostics: string[];
  runStatus: RunStatus;
  extensionRequest: ExtensionUiRequest | null;
};

export const disconnectedSnapshot: ConnectionSnapshot = {
  phase: "disconnected",
  workingDirectory: null,
  sessionId: null,
  sessionFile: null,
  model: null,
  thinkingLevel: null,
  error: null,
};

export const initialPiSessionState: PiSessionState = {
  connection: disconnectedSnapshot,
  messages: [],
  activeAssistantId: null,
  error: null,
  notice: null,
  diagnostics: [],
  runStatus: "idle",
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
        ...(event.snapshot.phase === "disconnected"
          ? { extensionRequest: null, runStatus: "idle" as const }
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
              runStatus: "idle" as const,
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
            kind: "message",
            content: event.message,
            thinking: "",
            blocks: [{ type: "text", contentIndex: 0, text: event.message }],
            tools: [],
          },
        ],
      };
    }
    case "assistant_message_started":
      return startAssistantMessage(state, envelope.sequence);
    case "assistant_text_delta":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        content: message.content + event.delta,
        blocks: appendBlockDelta(message.blocks, "text", event.contentIndex, event.delta),
      }));
    case "assistant_thinking_delta":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        thinking: message.thinking + event.delta,
        blocks: appendBlockDelta(message.blocks, "thinking", event.contentIndex, event.delta),
      }));
    case "assistant_message_completed":
      return updateActiveAssistant(state, envelope.sequence, (message) => ({
        ...message,
        content: event.text,
        thinking: event.thinking,
        blocks: event.blocks?.map((block) => ({
          type: block.kind,
          contentIndex: block.contentIndex,
          text: block.text,
        })) ?? message.blocks,
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
      return appendStatus(
        {
          ...state,
          connection: { ...state.connection, phase: "running" },
          runStatus: "running",
        },
        envelope.sequence,
        "running",
        "Pi 已开始执行任务",
      );
    case "run_finished":
      return state;
    case "run_settled": {
      const stopped = [...state.messages]
        .reverse()
        .find((message) => message.role === "assistant")?.stopReason === "aborted";
      const status: RunStatus = stopped ? "stopped" : state.error ? "failed" : "completed";
      return appendStatus(
        {
          ...state,
          connection: { ...state.connection, phase: "ready" },
          activeAssistantId: null,
          runStatus: status,
        },
        envelope.sequence,
        status,
        stopped ? "任务已由你停止，已完成的过程仍然保留" : "任务执行完成",
      );
    }
    case "retry_changed": {
      const content = event.active
        ? `正在进行第 ${event.attempt ?? "?"}/${event.maxAttempts ?? "?"} 次重试${event.delayMs ? `，将在 ${Math.ceil(event.delayMs / 1000)} 秒后开始` : ""}`
        : event.message ?? "重试已结束";
      return appendStatus(
        { ...state, notice: event.active ? content : event.message ?? null },
        envelope.sequence,
        event.active ? "running" : state.runStatus,
        content,
      );
    }
    case "compaction_changed":
      return updateCompaction(state, envelope.sequence, event);
    case "extension_ui_requested":
      return appendStatus(
        {
          ...state,
          runStatus: "waiting_input",
          extensionRequest: {
            requestId: event.requestId,
            method: event.method,
            title: event.title,
            message: event.message,
            options: event.options,
            placeholder: event.placeholder,
            defaultValue: event.defaultValue,
          },
        },
        envelope.sequence,
        "waiting_input",
        event.title ?? "Pi 需要你的输入",
      );
    case "extension_ui_resolved":
      return {
        ...state,
        runStatus: state.connection.phase === "running" ? "running" : state.runStatus,
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
      return appendStatus(
        {
          ...state,
          connection: { ...state.connection, phase: "failed", error },
          error,
          runStatus: "failed",
        },
        envelope.sequence,
        "failed",
        error.message,
      );
    }
    case "protocol_error": {
      const error = { code: "RPC_PROTOCOL_ERROR", message: event.message };
      return appendStatus(
        {
          ...state,
          connection: { ...state.connection, phase: "failed", error },
          error,
          runStatus: "failed",
        },
        envelope.sequence,
        "failed",
        "Pi RPC 通信失败",
      );
    }
  }
}

function appendStatus(
  state: PiSessionState,
  sequence: number,
  status: RunStatus,
  content: string,
): PiSessionState {
  return {
    ...state,
    messages: [
      ...state.messages,
      {
        id: `status-${sequence}`,
        role: "system",
        kind: "status",
        content,
        thinking: "",
        blocks: [],
        tools: [],
        status,
      },
    ],
  };
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
        kind: "message",
        content: "",
        thinking: "",
        blocks: [],
        tools: [],
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

function appendBlockDelta(
  blocks: MessageBlock[],
  type: MessageBlock["type"],
  contentIndex: number,
  delta: string,
): MessageBlock[] {
  const existing = blocks.find((block) => block.contentIndex === contentIndex);
  if (!existing) {
    return [...blocks, { type, contentIndex, text: delta }].sort(
      (left, right) => left.contentIndex - right.contentIndex,
    );
  }
  return blocks.map((block) =>
    block.contentIndex === contentIndex ? { ...block, type, text: block.text + delta } : block,
  );
}

function updateTool(
  state: PiSessionState,
  sequence: number,
  id: string,
  name: string,
  status: ToolRun["status"],
  details: { input?: string; output?: string; truncatedLines?: number | null },
): PiSessionState {
  return updateActiveAssistant(state, sequence, (message) => {
    const existing = message.tools.find((tool) => tool.id === id);
    const next: ToolRun = {
      id,
      name,
      status,
      input: details.input || existing?.input || "",
      output: details.output ?? existing?.output ?? "",
      truncatedLines: details.truncatedLines ?? existing?.truncatedLines ?? null,
    };
    const tools = existing
      ? message.tools.map((tool) => (tool.id === id ? next : tool))
      : [...message.tools, next];
    return { ...message, tools };
  });
}

function updateCompaction(
  state: PiSessionState,
  sequence: number,
  event: Extract<PiEvent, { type: "compaction_changed" }>,
): PiSessionState {
  if (event.active) {
    return { ...state, notice: "Compacting context…" };
  }
  if (event.aborted) {
    return { ...state, notice: null };
  }
  return {
    ...state,
    notice: null,
    messages: [
      ...state.messages,
      {
        id: `compaction-${sequence}`,
        role: "system",
        kind: "compaction",
        content: event.summary ?? "",
        thinking: "",
        blocks: [],
        tools: [],
        tokensBefore: event.tokensBefore,
        errorMessage: event.errorMessage,
        pending: false,
      },
    ],
  };
}
