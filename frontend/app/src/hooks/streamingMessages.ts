export interface UiTool {
  id: string;
  name: string;
  status: "running" | "complete";
}

export interface UiMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  /** reasoning 是否已流完（首次 content delta 或终帧后置位）。 */
  reasoningDone?: boolean;
  tools?: UiTool[];
  streaming?: boolean;
  /** 新生成回答的打字机揭示标记：流式结束后仍揭示至全文（历史消息无此标记，立即全显）。 */
  typewriter?: boolean;
}

type IdFactory = () => string;

function assistantForActivity(
  messages: UiMessage[],
  nextId: IdFactory,
  streaming: boolean,
): [UiMessage[], UiMessage] {
  const last = messages.at(-1);
  if (last?.role === "assistant" && (last.streaming || !streaming)) {
    return [messages.slice(0, -1), last];
  }
  return [
    messages,
    {
      id: nextId(),
      role: "assistant",
      content: "",
      ...(streaming ? { streaming: true, typewriter: true } : {}),
    },
  ];
}

export function appendAssistantContent(
  messages: UiMessage[],
  text: string,
  nextId: IdFactory,
): UiMessage[] {
  const last = messages.at(-1);
  if (last?.role === "assistant" && last.streaming) {
    return [
      ...messages.slice(0, -1),
      { ...last, content: last.content + text, reasoningDone: true, typewriter: true },
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
      reasoningDone: true,
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

export function appendAssistantTool(
  messages: UiMessage[],
  name: string,
  nextId: IdFactory,
  streaming = true,
): UiMessage[] {
  const [head, assistant] = assistantForActivity(messages, nextId, streaming);
  const tools = assistant.tools ?? [];
  return [
    ...head,
    {
      ...assistant,
      ...(streaming ? { streaming: true, typewriter: true } : {}),
      tools: [
        ...tools,
        {
          id: `${assistant.id}-tool-${tools.length + 1}`,
          name,
          status: streaming ? "running" : "complete",
        },
      ],
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
        tools: last.tools?.map((tool) => ({ ...tool, status: "complete" })),
        reasoningDone: true,
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
