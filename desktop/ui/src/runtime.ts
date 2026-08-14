//! assistant-ui 自定义 runtime 适配器：把 lure 的 WS turn 消息映射为
//! assistant-ui 的 ThreadMessageLike（reasoning / text / tool-call 三段 part）。

import { useExternalStoreRuntime } from "@assistant-ui/react";

/// lure 侧的一条消息（对齐 provider::LlmResponse 的 reasoning/content/tool_calls）。
export interface LureMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  reasoningContent?: string;
  toolCalls?: readonly { id: string; name: string; arguments: string }[];
}

export type LureMessagePart =
  | { type: "reasoning"; text: string }
  | { type: "text"; text: string }
  | { type: "tool-call"; toolCallId?: string; toolName: string; argsText?: string };

/// 把 lure 消息转换为 assistant-ui 的 message-like（content 为 part 数组）。
export function convertLureMessage(message: LureMessage): {
  id: string;
  role: "user" | "assistant";
  content: LureMessagePart[];
} {
  const content: LureMessagePart[] = [];
  if (message.reasoningContent) {
    content.push({ type: "reasoning", text: message.reasoningContent });
  }
  if (message.content) {
    content.push({ type: "text", text: message.content });
  }
  for (const tc of message.toolCalls ?? []) {
    content.push({ type: "tool-call", toolCallId: tc.id, toolName: tc.name, argsText: tc.arguments });
  }
  return { id: message.id, role: message.role, content };
}

/// 基于外部 store 的 runtime 适配器：`messages` 为 lure 消息，`onNew` 回传用户输入文本。
export function useLureRuntime(input: {
  messages: readonly LureMessage[];
  isRunning?: boolean;
  onNew: (content: string) => Promise<void>;
}) {
  return useExternalStoreRuntime({
    messages: input.messages,
    isRunning: input.isRunning,
    convertMessage: convertLureMessage,
    onNew: async (message) => {
      const text = message.content
        .filter((p): p is { type: "text"; text: string } => p.type === "text")
        .map((p) => p.text)
        .join("");
      await input.onNew(text);
    },
  });
}
