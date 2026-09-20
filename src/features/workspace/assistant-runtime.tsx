import {
  AuiProvider,
  ExternalThread,
  useAui,
  type AppendMessage,
  type ExternalThreadMessage,
  type ThreadAssistantMessagePart,
  type ThreadUserMessagePart,
  type ToolCallMessagePart,
} from "@assistant-ui/react";
import { useMemo, type ReactNode } from "react";

import type { ImageAttachment } from "@/features/pi-connection/api";
import type {
  ConversationMessage,
  MessagePart,
} from "@/features/pi-connection/reducer";
import { toolPartToView } from "./presentation";

function parseToolArgs(input: string): ToolCallMessagePart["args"] {
  if (!input.trim()) return {};
  try {
    const parsed: unknown = JSON.parse(input);
    return parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)
      ? (parsed as ToolCallMessagePart["args"])
      : {};
  } catch {
    return {};
  }
}

function convertAssistantPart(
  part: MessagePart,
  isRunning: boolean,
): ThreadAssistantMessagePart {
  const status = { type: isRunning ? ("running" as const) : ("complete" as const) };
  if (part.type === "text") return { type: "text", text: part.text, status };
  if (part.type === "thinking") {
    return { type: "reasoning", text: part.text, status };
  }

  return {
    type: "tool-call",
    toolCallId: part.toolCallId,
    toolName: part.name,
    args: parseToolArgs(part.input),
    argsText: part.input,
    result: part.output || undefined,
    isError: part.status === "error",
    artifact: toolPartToView(part),
  };
}

function messageStatus(message: ConversationMessage, isRunning: boolean) {
  if (isRunning) return { type: "running" as const };
  if (message.errorMessage) {
    return {
      type: "incomplete" as const,
      reason: "error" as const,
      error: message.errorMessage,
    };
  }
  if (message.stopReason === "length") {
    return { type: "incomplete" as const, reason: "length" as const };
  }
  if (message.stopReason === "aborted") {
    return { type: "incomplete" as const, reason: "cancelled" as const };
  }
  return { type: "complete" as const, reason: "stop" as const };
}

export function convertPiMessage(
  message: ConversationMessage,
  isRunning: boolean,
): ExternalThreadMessage {
  // Pi 消息没有时间戳，界面也不展示时间；用固定值满足 assistant-ui 的消息契约。
  const createdAt = new Date(0);
  if (message.role === "user") {
    return {
      id: message.id,
      role: "user",
      content: message.parts.flatMap<ThreadUserMessagePart>((part) =>
        part.type === "text" ? [{ type: "text", text: part.text }] : [],
      ),
      attachments: [],
      createdAt,
      metadata: { custom: {} },
    };
  }

  return {
    id: message.id,
    role: "assistant",
    content: message.parts.map((part) => convertAssistantPart(part, isRunning)),
    status: messageStatus(message, isRunning),
    createdAt,
    metadata: {
      unstable_state: null,
      unstable_annotations: [],
      unstable_data: [],
      steps: [],
      custom: {},
    },
  };
}

type ComposerMessageContent = Pick<AppendMessage, "content" | "attachments"> & {
  role?: AppendMessage["role"];
};

export function readAppendMessageText(message: ComposerMessageContent): string {
  return message.content
    .flatMap((part) => (part.type === "text" ? [part.text] : []))
    .join("\n")
    .trim();
}

const DATA_IMAGE_PATTERN = /^data:([^;,]+);base64,(.+)$/s;

export function readAppendMessageImages(
  message: ComposerMessageContent,
): ImageAttachment[] {
  return (message.attachments ?? []).flatMap((attachment) =>
    (attachment.content ?? []).flatMap((part) => {
      if (part.type !== "image") return [];
      const match = DATA_IMAGE_PATTERN.exec(part.image);
      if (!match) return [];
      return [{ mimeType: match[1], data: match[2] }];
    }),
  );
}

export function PiAssistantRuntimeProvider({
  children,
  messages,
  activeAssistantId,
  isRunning,
  canSend,
  onNew,
  onCancel,
}: {
  children: ReactNode;
  messages: ConversationMessage[];
  activeAssistantId: string | null;
  isRunning: boolean;
  canSend: boolean;
  onNew: (message: AppendMessage) => Promise<void> | void;
  onCancel: () => void;
}) {
  const convertedMessages = useMemo(
    () =>
      messages.map((message) =>
        convertPiMessage(
          message,
          isRunning && message.role === "assistant" && message.id === activeAssistantId,
        ),
      ),
    [activeAssistantId, isRunning, messages],
  );
  const aui = useAui({
    thread: ExternalThread({
      messages: convertedMessages,
      isRunning,
      isSendDisabled: !canSend,
      onNew: (message) => {
        void Promise.resolve(onNew(message)).catch(() => undefined);
      },
      onCancel,
    }),
  });

  return <AuiProvider value={aui}>{children}</AuiProvider>;
}
