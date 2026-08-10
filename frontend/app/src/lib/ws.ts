// WebUI 复用协议 WS 客户端。
//
// 入站信封（发往后端）：{ type: "attach" | "new_chat" | "message", chat_id?, content? }
// 出站事件（后端 → 前端）：{ event, ... }
//   ready(chat_id, client_id) / attached(chat_id) / session_updated(chat_id)
//   goal_status / delta(text) / reasoning_delta(text) / message(text) / turn_end / error(detail)

export type ServerEvent =
  | { event: "ready"; chat_id: string; client_id: string }
  | { event: "attached"; chat_id: string }
  | { event: "session_updated"; chat_id: string }
  | { event: "goal_status"; chat_id: string; [k: string]: unknown }
  | { event: "delta"; chat_id: string; text: string }
  | { event: "reasoning_delta"; chat_id: string; text: string }
  | { event: "message"; chat_id: string; text: string }
  | { event: "turn_end"; chat_id: string }
  | { event: "error"; detail: string }
  | { event: string; [k: string]: unknown };

type Listener = (event: ServerEvent) => void;

export class ChatSocket {
  private ws: WebSocket | null = null;
  private listeners = new Set<Listener>();
  private queue: string[] = [];
  private ready = false;

  constructor(private readonly url: string) {}

  connect() {
    const ws = new WebSocket(this.url);
    this.ws = ws;
    ws.onopen = () => {
      this.ready = true;
      for (const frame of this.queue) ws.send(frame);
      this.queue = [];
    };
    ws.onmessage = (ev) => {
      let parsed: ServerEvent;
      try {
        parsed = JSON.parse(ev.data as string) as ServerEvent;
      } catch {
        return;
      }
      for (const l of this.listeners) l(parsed);
    };
    ws.onclose = () => {
      this.ready = false;
    };
  }

  on(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private send(frame: Record<string, unknown>) {
    const text = JSON.stringify(frame);
    if (this.ready && this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(text);
    } else {
      this.queue.push(text);
    }
  }

  attach(chatId: string) {
    this.send({ type: "attach", chat_id: chatId });
  }

  newChat() {
    this.send({ type: "new_chat" });
  }

  message(chatId: string, content: string) {
    this.send({ type: "message", chat_id: chatId, content });
  }

  close() {
    this.ws?.close();
    this.ws = null;
    this.listeners.clear();
  }
}
