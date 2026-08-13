export interface UiMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  streaming?: boolean;
  /** 新生成回答的打字机揭示标记：流式结束后仍揭示至全文（历史消息无此标记，立即全显）。 */
  typewriter?: boolean;
}

type IdFactory = () => string;

export function appendAssistantContent(
  messages: UiMessage[],
  text: string,
  nextId: IdFactory,
): UiMessage[] {
  const last = messages.at(-1);
  if (last?.role === "assistant" && last.streaming) {
    return [
      ...messages.slice(0, -1),
      { ...last, content: last.content + text, typewriter: true },
    ];
  }
  return [
    ...messages,
    {
      id: nextId(),
      role: "assistant",
      content: text,
      streaming: true,
      typewriter: true,
    },
  ];
}

export function appendAssistantReasoning(
  messages: UiMessage[],
  text: string,
  nextId: IdFactory,
): UiMessage[] {
  const last = messages.at(-1);
  if (last?.role === "assistant" && last.streaming) {
    return [
      ...messages.slice(0, -1),
      {
        ...last,
        reasoning: (last.reasoning ?? "") + text,
        typewriter: true,
      },
    ];
  }
  return [
    ...messages,
    {
      id: nextId(),
      role: "assistant",
      content: "",
      reasoning: text,
      streaming: true,
      typewriter: true,
    },
  ];
}

export function finalizeAssistant(
  messages: UiMessage[],
  fullText: string | undefined,
  nextId: IdFactory,
): UiMessage[] {
  const last = messages.at(-1);
  if (last?.role === "assistant" && last.streaming) {
    return [
      ...messages.slice(0, -1),
      {
        ...last,
        content: fullText ?? last.content,
        streaming: false,
        typewriter: true,
      },
    ];
  }
  if (fullText == null) return messages;
  return [
    ...messages,
    {
      id: nextId(),
      role: "assistant",
      content: fullText,
      typewriter: true,
    },
  ];
}
