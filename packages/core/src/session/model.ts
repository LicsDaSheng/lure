//! 会话内存模型 `Session`（对齐 crates/lure-core/src/session/model.rs）。

import type { Json } from "../provider/types.js";

/// 单文件历史默认最大消息条数。
export const FILE_MAX_MESSAGES = 2000;

/// 一个会话。
export class Session {
  messages: Json[];
  metadata: Record<string, Json>;
  createdAt: string;
  updatedAt: string;
  private lastConsolidated: number;

  constructor(
    readonly key: string,
    messages: Json[] = [],
    metadata: Record<string, Json> = {},
    createdAt = new Date().toISOString(),
    updatedAt = new Date().toISOString(),
    lastConsolidated = 0,
  ) {
    this.messages = messages;
    this.metadata = metadata;
    this.createdAt = createdAt;
    this.updatedAt = updatedAt;
    this.lastConsolidated = clampOffset(lastConsolidated, messages.length);
  }

  /// 从已加载数据构造，并对 `last_consolidated` 做 clamp。
  static fromLoaded(
    key: string,
    messages: Json[],
    metadata: Record<string, Json>,
    createdAt: string,
    updatedAt: string,
    lastConsolidated: Json,
  ): Session {
    return new Session(key, messages, metadata, createdAt, updatedAt, clampOffset(lastConsolidated, messages.length));
  }

  lastConsolidatedOffset(): number {
    return this.lastConsolidated;
  }

  /// 追加一条消息并刷新更新时间。
  addMessage(role: string, content: string): void {
    this.addMessageWith(role, content, {});
  }

  /// 追加一条带附加字段的消息（如 assistant 的 `reasoning_content`）。
  addMessageWith(role: string, content: string, extra: Record<string, Json>): void {
    const msg: Record<string, Json> = {
      role,
      content,
      timestamp: new Date().toISOString(),
    };
    for (const [k, v] of Object.entries(extra)) {
      if (k !== "role" && k !== "content" && k !== "timestamp") msg[k] = v;
    }
    this.messages.push(msg);
    this.updatedAt = new Date().toISOString();
  }

  /// 返回未整合消息的近程窗口。
  getHistory(maxMessages: number): Json[] {
    const unconsolidated = this.messages.slice(this.lastConsolidated);
    const max = maxMessages === 0 ? FILE_MAX_MESSAGES : maxMessages;
    const start = Math.max(0, unconsolidated.length - max);
    return unconsolidated.slice(start);
  }

  /// 清空会话并重置状态。
  clear(): void {
    this.messages = [];
    this.lastConsolidated = 0;
    this.updatedAt = new Date().toISOString();
    delete this.metadata["_last_summary"];
  }
}

/// 只有落在 `[0, len]` 内的非负整数才是合法偏移；其余重置为 0。
function clampOffset(raw: Json, len: number): number {
  if (typeof raw === "number" && Number.isInteger(raw) && raw >= 0 && raw <= len) {
    return raw;
  }
  return 0;
}
