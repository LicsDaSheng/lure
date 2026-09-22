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

export type ProjectDescriptor = {
  name: string;
  directory: string;
};

export type RecentConversation = {
  sessionId: string;
  title: string;
  updatedAt: number;
};

/** Pi 会话文件的只读摘要，来自桌面端对 Pi 会话目录的扫描。 */
export type PiSessionSummary = {
  path: string;
  id: string;
  cwd: string | null;
  name: string | null;
  parentSessionPath: string | null;
  createdAtMs: number;
  modifiedAtMs: number;
  messageCount: number;
  firstMessage: string | null;
};

export type PiSessionPage = {
  sessions: PiSessionSummary[];
  hasMore: boolean;
};

/** Pi 会话条目的消息主体；桌面端只读取展示所需字段。 */
export type SessionEntryMessage = {
  role?: string;
  content?: unknown;
  timestamp?: number;
  stopReason?: string | null;
  errorMessage?: string | null;
  toolCallId?: string;
  toolName?: string;
  isError?: boolean;
  details?: unknown;
};

/** Pi 会话文件中的一条原始条目（消息、会话信息、压缩摘要等）。 */
export type SessionEntry = {
  type?: string;
  id?: string;
  parentId?: string | null;
  timestamp?: string;
  name?: string;
  message?: SessionEntryMessage;
};

/** `get_entries` 的响应：当前会话的全部条目与活动分支叶子。 */
export type SessionEntries = {
  entries: SessionEntry[];
  leafId: string | null;
};

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

export type TextPart = {
  id: string;
  type: "text";
  contentIndex: number;
  text: string;
};

export type ThinkingPart = {
  id: string;
  type: "thinking";
  contentIndex: number;
  text: string;
};

/** 工具调用是助手内容里的一个 part，位置由它在 Pi 内容序列中的位置决定。 */
/** 工具执行在界面上的三种可见状态。 */
export type ToolStatus = "running" | "completed" | "error";

export type ToolPart = {
  id: string;
  type: "tool";
  contentIndex: number;
  toolCallId: string;
  name: string;
  status: ToolStatus;
  input: string;
  output: string;
  truncatedLines: number | null;
};

/**
 * 助手消息的单一有序内容序列。
 *
 * 文本、思考与工具调用按 Pi 的真实输出顺序交错保存，渲染层不再需要
 * 把工具堆到消息末尾。工具没有自己的 contentIndex 事件，位置按
 * “当前已见最大索引的下一位” 推断，并与 message_end 的权威内容对齐。
 */
export type MessagePart = TextPart | ThinkingPart | ToolPart;

export type ConversationMessage = {
  id: string;
  role: "assistant" | "user";
  parts: MessagePart[];
  stopReason?: string | null;
  errorMessage?: string | null;
};

export function messageText(message: ConversationMessage): string {
  return message.parts.flatMap((part) => (part.type === "text" ? [part.text] : [])).join("");
}

export function messageThinking(message: ConversationMessage): string {
  return message.parts
    .flatMap((part) => (part.type === "thinking" ? [part.text] : []))
    .join("");
}

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
