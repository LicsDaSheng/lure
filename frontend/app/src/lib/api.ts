// 后端 HTTP 契约（对齐 lure-core::webui）。传输无关的纯数据获取层。

export interface Bootstrap {
  token: string;
  api_token: string;
  ws_path: string;
  ws_url: string;
  expires_in: number;
  model_name: string | null;
}

export interface SessionRow {
  key: string;
  created_at: string;
  updated_at: string;
  title: string;
  preview: string;
}

export interface ThreadMessage {
  role: string;
  content: unknown;
  reasoning_content?: string;
}

export interface Thread {
  key: string;
  messages: ThreadMessage[];
}

async function getJson<T>(url: string, token?: string): Promise<T> {
  const headers: Record<string, string> = {};
  if (token) headers["Authorization"] = `Bearer ${token}`;
  const res = await fetch(url, { headers });
  if (!res.ok) {
    throw new Error(`${res.status} ${res.statusText} @ ${url}`);
  }
  return (await res.json()) as T;
}

/** `GET /webui/bootstrap`：拿 token 与 ws_url（绝对地址）。 */
export function fetchBootstrap(): Promise<Bootstrap> {
  return getJson<Bootstrap>("/webui/bootstrap");
}

/** `GET /api/sessions`：会话列表（后端已按 updated_at 倒序）。 */
export async function fetchSessions(token: string): Promise<SessionRow[]> {
  const data = await getJson<{ sessions: SessionRow[] }>("/api/sessions", token);
  return data.sessions ?? [];
}

/** `GET /api/sessions/<key>/webui-thread`：某会话的历史消息。 */
export function fetchThread(key: string, token: string): Promise<Thread> {
  const encoded = encodeURIComponent(key);
  return getJson<Thread>(`/api/sessions/${encoded}/webui-thread`, token);
}

/** thread 消息 content 可能是字符串或结构化片段，统一投影为可显示文本。 */
export function messageText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((part) =>
        part && typeof part === "object" && "text" in part
          ? String((part as { text: unknown }).text ?? "")
          : typeof part === "string"
            ? part
            : "",
      )
      .join("");
  }
  return "";
}
