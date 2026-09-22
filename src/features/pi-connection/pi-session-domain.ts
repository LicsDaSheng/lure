import type {
  ConnectionSnapshot,
  ConversationMessage,
  EventEnvelope,
  MessagePart,
  PiSessionState,
  RunState,
  SessionEntry,
  ToolPart,
} from "./pi-session-types";

/**
 * 把 Pi 会话条目重建为可展示的历史对话。
 *
 * 条目是 append-only 树，这里只沿 `leafId` 回溯出的活动分支重建，
 * 并让 `toolResult` 回填到同一 `toolCallId` 的工具 part，保持 Pi 记录的内容顺序。
 */
export function conversationFromEntries(
  entries: SessionEntry[],
  leafId: string | null,
): ConversationMessage[] {
  const messages: ConversationMessage[] = [];
  const tools = new Map<string, ToolPart>();

  for (const entry of activeBranch(entries, leafId)) {
    const message = entry.message;
    if (entry.type !== "message" || !message) continue;

    if (message.role === "user") {
      messages.push({
        id: `user-${entry.id}`,
        role: "user",
        parts: [
          { id: "text-0", type: "text", contentIndex: 0, text: contentText(message.content) },
        ],
      });
      continue;
    }

    if (message.role === "assistant") {
      const parts = assistantParts(message.content);
      for (const part of parts) {
        if (part.type === "tool") tools.set(part.toolCallId, part);
      }
      messages.push({
        id: `assistant-${entry.id}`,
        role: "assistant",
        parts,
        stopReason: message.stopReason ?? null,
        errorMessage: message.errorMessage ?? null,
      });
      continue;
    }

    if (message.role === "toolResult" && message.toolCallId) {
      const part = tools.get(message.toolCallId);
      if (!part) continue;
      part.output = contentText(message.content);
      part.truncatedLines = truncatedLines(message.details);
      part.status = message.isError ? "error" : "completed";
    }
  }

  return messages;
}

/** 沿 parentId 回溯活动分支；缺少叶子信息时保守地按追加顺序返回。 */
function activeBranch(entries: SessionEntry[], leafId: string | null): SessionEntry[] {
  if (!leafId) return entries;

  const byId = new Map<string, SessionEntry>();
  for (const entry of entries) {
    if (entry.id) byId.set(entry.id, entry);
  }

  const branch: SessionEntry[] = [];
  const visited = new Set<string>();
  let cursor: string | null | undefined = leafId;
  while (cursor && !visited.has(cursor)) {
    visited.add(cursor);
    const entry = byId.get(cursor);
    if (!entry) break;
    branch.push(entry);
    cursor = entry.parentId ?? null;
  }
  return branch.reverse();
}

function assistantParts(content: unknown): MessagePart[] {
  if (!Array.isArray(content)) return [];

  const parts: MessagePart[] = [];
  content.forEach((block, contentIndex) => {
    if (!isRecord(block)) return;
    if (block.type === "text" && typeof block.text === "string") {
      parts.push({ id: `text-${contentIndex}`, type: "text", contentIndex, text: block.text });
      return;
    }
    if (block.type === "thinking" && typeof block.thinking === "string") {
      parts.push({
        id: `thinking-${contentIndex}`,
        type: "thinking",
        contentIndex,
        text: block.thinking,
      });
      return;
    }
    if (block.type === "toolCall" && typeof block.id === "string") {
      // 历史条目里的工具调用已经结束，状态按“已完成”展示，等待 toolResult 回填输出。
      parts.push({
        id: block.id,
        type: "tool",
        contentIndex,
        toolCallId: block.id,
        name: typeof block.name === "string" ? block.name : "",
        status: "completed",
        input: formatInput(block.arguments),
        output: "",
        truncatedLines: null,
      });
    }
  });
  return parts;
}

/** 消息内容既可能是纯文本，也可能是文本与图片块混排。 */
function contentText(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .flatMap((block) => (isRecord(block) && block.type === "text" && typeof block.text === "string" ? [block.text] : []))
    .join("\n");
}

function formatInput(arguments_: unknown): string {
  if (arguments_ === undefined || arguments_ === null) return "";
  try {
    return JSON.stringify(arguments_, null, 2) ?? "";
  } catch {
    return "";
  }
}

/** 读取 Pi 在工具结果里记录的截断行数，与 RPC 归一化保持同一口径。 */
function truncatedLines(details: unknown): number | null {
  if (!isRecord(details) || !isRecord(details.truncation)) return null;
  const { truncated, totalLines, outputLines } = details.truncation;
  if (truncated !== true || typeof totalLines !== "number" || typeof outputLines !== "number") {
    return null;
  }
  return Math.max(totalLines - outputLines, 0);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

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
