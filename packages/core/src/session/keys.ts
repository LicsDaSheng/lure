//! Session key 常量与助手（对齐 crates/lure-core/src/session/keys.rs）。

/// 统一会话 key。
export const UNIFIED_SESSION_KEY = "unified:default";

/// 返回 channel/chat 对应的 session key。
export function sessionKeyForChannel(channel: string, chatId: string, unifiedSession: boolean): string {
  return unifiedSession ? UNIFIED_SESSION_KEY : `${channel}:${chatId}`;
}
