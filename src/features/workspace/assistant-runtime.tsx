import {
  AssistantRuntimeProvider,
  type AppendMessage,
  type ThreadMessageLike,
  useExternalMessageConverter,
  useExternalStoreRuntime,
} from "@assistant-ui/react";
import { useCallback, type ReactNode } from "react";

import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";

type AssistantContentPart = Exclude<ThreadMessageLike["content"], string>[number];
type AssistantToolCallPart = Extract<AssistantContentPart, { type: "tool-call" }>;

function parseToolArguments(input: string): NonNullable<AssistantToolCallPart["args"]> {
  if (!input.trim()) return {};
  try {
    const parsed: unknown = JSON.parse(input);
    return parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as NonNullable<AssistantToolCallPart["args"]>)
      : {};
  } catch {
    return {};
  }
}

function convertTool(tool: ToolRun) {
  return {
    type: "tool-call" as const,
    toolCallId: tool.id,
    toolName: tool.name,
    args: parseToolArguments(tool.input),
    argsText: tool.input,
    ...(tool.status === "running" ? {} : { result: tool.output }),
    isError: tool.status === "error",
  };
}

function getAssistantStatus(message: ConversationMessage, isActive: boolean) {
  if (isActive) return { type: "running" as const };
  if (message.errorMessage) {
    return {
      type: "incomplete" as const,
      reason: "error" as const,
      error: message.errorMessage,
    };
  }
  if (message.stopReason === "aborted") {
    return { type: "incomplete" as const, reason: "cancelled" as const };
  }
  if (message.stopReason === "length") {
    return { type: "incomplete" as const, reason: "length" as const };
  }
  return { type: "complete" as const, reason: "stop" as const };
}

export function convertPiMessage(
  message: ConversationMessage,
  isActive: boolean,
): ThreadMessageLike {
  const partStatus = { type: isActive ? ("running" as const) : ("complete" as const) };
  const content: AssistantContentPart[] = message.blocks.map((block) => ({
    type: block.type === "thinking" ? ("reasoning" as const) : ("text" as const),
    text: block.text,
    status: partStatus,
  }));

  if (content.length === 0 && message.content) {
    content.push({ type: "text", text: message.content, status: partStatus });
  }
  content.push(...message.tools.map(convertTool));

  return {
    id: message.id,
    role: message.role,
    content,
    ...(message.role === "assistant"
      ? { status: getAssistantStatus(message, isActive) }
      : {}),
    metadata: { custom: { sourceMessageId: message.id } },
  };
}

export function readAppendMessageText(
  message: Pick<AppendMessage, "role" | "content">,
): string {
  return message.content
    .filter((part): part is Extract<(typeof message.content)[number], { type: "text" }> =>
      part.type === "text",
    )
    .map((part) => part.text)
    .join("\n")
    .trim();
}

export function PiAssistantRuntimeProvider({
  children,
  messages,
  isRunning,
  isSendDisabled,
  activeAssistantId,
  onNew,
  onCancel,
}: {
  children: ReactNode;
  messages: ConversationMessage[];
  isRunning: boolean;
  isSendDisabled: boolean;
  activeAssistantId: string | null;
  onNew: (message: AppendMessage) => Promise<void>;
  onCancel: () => Promise<void>;
}) {
  const convertMessage = useCallback(
    (message: ConversationMessage) =>
      convertPiMessage(message, isRunning && message.id === activeAssistantId),
    [activeAssistantId, isRunning],
  );
  const convertedMessages = useExternalMessageConverter({
    callback: convertMessage,
    isRunning,
    joinStrategy: "none",
    messages,
  });
  const runtime = useExternalStoreRuntime({
    messages: convertedMessages,
    isRunning,
    isSendDisabled,
    onNew,
    onCancel,
  });

  return <AssistantRuntimeProvider runtime={runtime}>{children}</AssistantRuntimeProvider>;
}
