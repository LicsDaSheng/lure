import * as React from "react";
import {
  fetchBootstrap,
  fetchSessions,
  fetchThread,
  messageText,
  type Bootstrap,
  type SessionRow,
} from "@/lib/api";
import { ChatSocket, type ServerEvent } from "@/lib/ws";
import {
  appendAssistantContent,
  appendAssistantReasoning,
  appendAssistantTool,
  finalizeAssistant,
  type UiMessage,
} from "./streamingMessages";

export type { UiMessage } from "./streamingMessages";

const WS_PREFIX = "websocket:";

export type ConnState = "connecting" | "ready" | "error";

/** session key (`websocket:<uuid>`) ↔ WS chat_id (`<uuid>`)。 */
const keyToChatId = (key: string) =>
  key.startsWith(WS_PREFIX) ? key.slice(WS_PREFIX.length) : key;
const chatIdToKey = (chatId: string) => `${WS_PREFIX}${chatId}`;

let msgSeq = 0;
const nextId = () => `m${Date.now()}-${msgSeq++}`;

export function useChat() {
  const [conn, setConn] = React.useState<ConnState>("connecting");
  const [boot, setBoot] = React.useState<Bootstrap | null>(null);
  const [sessions, setSessions] = React.useState<SessionRow[]>([]);
  const [activeKey, setActiveKey] = React.useState<string | null>(null);
  const [messages, setMessages] = React.useState<UiMessage[]>([]);
  const [streaming, setStreaming] = React.useState(false);

  const socketRef = React.useRef<ChatSocket | null>(null);
  const tokenRef = React.useRef<string>("");
  const activeChatIdRef = React.useRef<string | null>(null);
  const defaultChatIdRef = React.useRef<string | null>(null);

  const reloadSessions = React.useCallback(async () => {
    try {
      setSessions(await fetchSessions(tokenRef.current));
    } catch (e) {
      console.error("加载会话列表失败", e);
    }
  }, []);

  // 仅处理当前活动会话的流式事件；chat_id 不匹配则忽略。
  const handleEvent = React.useCallback(
    (ev: ServerEvent) => {
      switch (ev.event) {
        case "ready":
          defaultChatIdRef.current = ev.chat_id as string;
          break;
        case "attached":
          break;
        case "session_updated":
          void reloadSessions();
          break;
        case "delta": {
          if (ev.chat_id !== activeChatIdRef.current) return;
          setMessages((prev) =>
            appendAssistantContent(prev, ev.text as string, nextId),
          );
          break;
        }
        case "reasoning_delta": {
          if (ev.chat_id !== activeChatIdRef.current) return;
          setMessages((prev) =>
            appendAssistantReasoning(prev, ev.text as string, nextId),
          );
          break;
        }
        case "tool_invoked": {
          if (ev.chat_id !== activeChatIdRef.current) return;
          setMessages((prev) =>
            appendAssistantTool(prev, ev.name as string, nextId),
          );
          break;
        }
        case "message": {
          if (ev.chat_id !== activeChatIdRef.current) return;
          setMessages((prev) =>
            finalizeAssistant(prev, ev.text as string, nextId),
          );
          break;
        }
        case "turn_end":
          if (ev.chat_id !== activeChatIdRef.current) return;
          setStreaming(false);
          break;
        case "error":
          setStreaming(false);
          setMessages((prev) =>
            finalizeAssistant(
              appendAssistantContent(
                prev,
                `⚠️ ${String((ev as { detail?: string }).detail ?? "unknown error")}`,
                nextId,
              ),
              undefined,
              nextId,
            ),
          );
          break;
      }
    },
    [reloadSessions],
  );

  React.useEffect(() => {
    let disposed = false;
    (async () => {
      try {
        const b = await fetchBootstrap();
        if (disposed) return;
        setBoot(b);
        // HTTP `/api/*` 用 api_token；WS 握手用 token（?token= query）。
        tokenRef.current = b.api_token;
        const sep = b.ws_url.includes("?") ? "&" : "?";
        const wsUrl = `${b.ws_url}${sep}token=${encodeURIComponent(b.token)}`;
        const socket = new ChatSocket(wsUrl);
        socketRef.current = socket;
        socket.on(handleEvent);
        socket.connect();
        setConn("ready");
        await reloadSessions();
      } catch (e) {
        console.error("bootstrap 失败", e);
        setConn("error");
      }
    })();
    return () => {
      disposed = true;
      socketRef.current?.close();
      socketRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectSession = React.useCallback(async (key: string) => {
    const chatId = keyToChatId(key);
    activeChatIdRef.current = chatId;
    setActiveKey(key);
    setStreaming(false);
    socketRef.current?.attach(chatId);
    try {
      const thread = await fetchThread(key, tokenRef.current);
      setMessages(
        thread.messages
          .filter((m) => m.role === "user" || m.role === "assistant")
          .map((m) => {
            const id = nextId();
            return {
              id,
              role: m.role as "user" | "assistant",
              content: messageText(m.content),
              reasoning: m.reasoning_content,
              reasoningDone: true,
              tools: m.tools
                ?.filter((tool) => typeof tool.name === "string" && tool.name)
                .map((tool, index) => ({
                  id: `${id}-tool-${index + 1}`,
                  name: tool.name,
                  status: "complete" as const,
                })),
            };
          }),
      );
    } catch (e) {
      console.error("加载会话消息失败", e);
      setMessages([]);
    }
  }, []);

  const newChat = React.useCallback(() => {
    // 后端 new_chat 返回新 chat_id 并 session_updated；我们乐观地用 default 兜底，
    // 但真正的 chat_id 以后续 attached 为准。这里直接清空并等用户发第一条。
    const socket = socketRef.current;
    if (!socket) return;
    let settled = false;
    const off = socket.on((ev) => {
      if (ev.event === "attached" && !settled) {
        settled = true;
        const chatId = ev.chat_id as string;
        activeChatIdRef.current = chatId;
        setActiveKey(chatIdToKey(chatId));
        off();
      }
    });
    activeChatIdRef.current = null;
    setActiveKey(null);
    setMessages([]);
    setStreaming(false);
    socket.newChat();
  }, []);

  const send = React.useCallback((content: string) => {
    const socket = socketRef.current;
    const chatId = activeChatIdRef.current ?? defaultChatIdRef.current;
    if (!socket || !chatId) return;
    activeChatIdRef.current = chatId;
    setActiveKey((k) => k ?? chatIdToKey(chatId));
    setMessages((prev) => [
      ...prev,
      { id: nextId(), role: "user", content },
    ]);
    setStreaming(true);
    socket.message(chatId, content);
  }, []);

  return {
    conn,
    apiToken: boot?.api_token ?? "",
    modelName: boot?.model_name ?? null,
    sessions,
    activeKey,
    messages,
    streaming,
    selectSession,
    newChat,
    send,
  };
}
