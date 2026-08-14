//! WebUI WebSocket 协议：入站解析与出站事件（传输无关，对齐 webui/ws.rs）。

import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";

/// 出站 `message` 事件：完整回复。
export function messageEvent(chatId: string, text: string): Json {
  return { event: "message", chat_id: chatId, text };
}

/// 出站 `delta` 事件：流式增量。
export function deltaEvent(chatId: string, text: string): Json {
  return { event: "delta", chat_id: chatId, text };
}

/// 出站 `status` 事件。
export function statusEvent(status: string): Json {
  return { event: "status", status };
}

/// 出站 `error` 事件。
export function errorEvent(detail: string): Json {
  return { event: "error", detail };
}

export type ParsedInbound =
  | { ok: true; chatId: string; content: string }
  | { ok: false; detail: string };

/// 解析入站聊天帧；校验 chat_id 与 content。
export function parseWsInbound(frame: Json, _channel: string): ParsedInbound {
  const obj = asObject(frame) ?? {};
  const chatId = asString(obj["chat_id"]) ?? "";
  if (chatId === "") return { ok: false, detail: "invalid chat_id" };
  const content = asString(obj["content"]) ?? "";
  if (content === "") return { ok: false, detail: "missing content" };
  return { ok: true, chatId, content };
}
