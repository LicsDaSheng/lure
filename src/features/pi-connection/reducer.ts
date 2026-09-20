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

export type RunPhase = "idle" | "running" | "waiting_input" | "retrying" | "compacting";

export type RunState = {
  phase: RunPhase;
  retry: {
    active: boolean;
    attempt: number | null;
    maxAttempts: number | null;
    delayMs: number | null;
    message: string | null;
  } | null;
  compaction: {
    active: boolean;
    reason: string | null;
    aborted: boolean | null;
    summary: string | null;
    tokensBefore: number | null;
    errorMessage: string | null;
  } | null;
};

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
  role: "assistant" | "user";
  content: string;
  thinking: string;
  blocks: MessageBlock[];
  tools: ToolRun[];
  stopReason?: string | null;
  errorMessage?: string | null;
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
  run: RunState;
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
