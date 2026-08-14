//! 会话存储与内存缓存 `SessionManager`（对齐 crates/lure-core/src/session/store.rs）。

import fs from "node:fs";
import path from "node:path";
import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";
import { Session } from "./model.js";

export const SESSION_CACHE_MAX_SIZE = 128;

export class SessionError extends Error {
  constructor(
    readonly kind: "io" | "not_cached",
    message: string,
    readonly key?: string,
  ) {
    super(message);
    this.name = "SessionError";
  }
}

export class SessionManager {
  private readonly cache = new Map<string, Session>();
  private lru: string[] = [];
  private maxCached = SESSION_CACHE_MAX_SIZE;

  constructor(private readonly sessionsDir: string) {
    fs.mkdirSync(sessionsDir, { recursive: true });
  }

  static forWorkspace(workspace: string): SessionManager {
    return new SessionManager(path.join(workspace, "sessions"));
  }

  sessionsDirPath(): string {
    return this.sessionsDir;
  }

  setMaxCached(limit: number): void {
    this.maxCached = limit;
  }

  cacheLen(): number {
    return this.cache.size;
  }

  cacheKeys(): string[] {
    return [...this.lru];
  }

  /// base64url（无 padding）编码存储 key。
  static storageKey(key: string): string {
    return Buffer.from(key, "utf8").toString("base64url");
  }

  static decodeStorageKey(stem: string): string | undefined {
    try {
      return Buffer.from(stem, "base64url").toString("utf8");
    } catch {
      return undefined;
    }
  }

  sessionPath(key: string): string {
    return path.join(this.sessionsDir, `${SessionManager.storageKey(key)}.jsonl`);
  }

  /// 删除已存储会话（磁盘 + 缓存）；文件不存在返回 `false`。
  deleteStored(key: string): boolean {
    this.cache.delete(key);
    this.lru = this.lru.filter((k) => k !== key);
    const p = this.sessionPath(key);
    if (!fs.existsSync(p)) return false;
    fs.unlinkSync(p);
    return true;
  }

  /// 枚举 sessions 目录下所有已存储会话 key。
  listStoredKeys(): string[] {
    let entries: string[];
    try {
      entries = fs.readdirSync(this.sessionsDir);
    } catch {
      return [];
    }
    const keys = entries
      .filter((e) => e.endsWith(".jsonl"))
      .map((e) => SessionManager.decodeStorageKey(e.slice(0, -".jsonl".length)))
      .filter((k): k is string => k !== undefined);
    keys.sort();
    return [...new Set(keys)];
  }

  /// 命中缓存返回；否则加载或新建。
  getOrCreate(key: string): Session {
    const cached = this.cache.get(key);
    if (cached !== undefined) {
      this.touch(key);
      return cached;
    }
    const session = this.load(key) ?? new Session(key);
    this.insert(key, session);
    return this.cache.get(key)!;
  }

  /// 将缓存中该 key 的会话原子写入磁盘。
  save(key: string, fsync: boolean): void {
    const session = this.cache.get(key);
    if (session === undefined) {
      throw new SessionError("not_cached", `会话未缓存: ${key}`, key);
    }
    writeSessionToDisk(this.sessionPath(key), session, fsync);
    this.touch(key);
  }

  /// 重存所有缓存会话（带 fsync），返回成功数；单条失败不影响其余。
  flushAll(): number {
    let flushed = 0;
    for (const key of [...this.cache.keys()]) {
      try {
        this.save(key, true);
        flushed += 1;
      } catch {
        // 单条失败不影响其余。
      }
    }
    return flushed;
  }

  /// 从内存缓存移除会话（不删磁盘）。
  invalidate(key: string): void {
    this.cache.delete(key);
    this.lru = this.lru.filter((k) => k !== key);
  }

  private load(key: string): Session | undefined {
    const p = this.sessionPath(key);
    let text: string;
    try {
      text = fs.readFileSync(p, "utf8");
    } catch (e) {
      if ((e as NodeJS.ErrnoException).code === "ENOENT") return undefined;
      throw new SessionError("io", `会话文件 IO 失败 ${p}: ${e}`);
    }
    return parseSession(key, text);
  }

  private insert(key: string, session: Session): void {
    this.lru = this.lru.filter((k) => k !== key);
    this.cache.set(key, session);
    this.lru.push(key);
    while (this.cache.size > this.maxCached) {
      const evicted = this.lru.shift();
      if (evicted === undefined) break;
      this.cache.delete(evicted);
    }
  }

  private touch(key: string): void {
    const pos = this.lru.indexOf(key);
    if (pos >= 0) {
      this.lru.splice(pos, 1);
      this.lru.push(key);
    }
  }
}

/// 容错解析 JSONL：跳过无法解析或非对象的行；首个 metadata 行提供会话元数据。
function parseSession(key: string, text: string): Session {
  let messages: Json[] = [];
  let metadata: Record<string, Json> = {};
  let createdAt: string | undefined;
  let updatedAt: string | undefined;
  let lastConsolidated: Json = 0;

  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed === "") continue;
    let obj: Json;
    try {
      obj = JSON.parse(trimmed);
    } catch {
      continue;
    }
    const o = asObject(obj);
    if (o === undefined) continue;
    if (asString(o["_type"]) === "metadata") {
      metadata = asObject(o["metadata"]) ?? {};
      createdAt = asString(o["created_at"]);
      updatedAt = asString(o["updated_at"]);
      if (o["last_consolidated"] !== undefined) lastConsolidated = o["last_consolidated"];
    } else {
      messages.push(obj);
    }
  }

  const now = new Date().toISOString();
  return Session.fromLoaded(key, messages, metadata, createdAt ?? now, updatedAt ?? now, lastConsolidated);
}

/// temp + rename 原子写会话 JSONL；`fsync` 时刷 file。
function writeSessionToDisk(p: string, session: Session, fsync: boolean): void {
  const tmp = `${p}.tmp`;
  try {
    const metaLine = JSON.stringify({
      _type: "metadata",
      key: session.key,
      created_at: session.createdAt,
      updated_at: session.updatedAt,
      metadata: session.metadata,
      last_consolidated: session.lastConsolidatedOffset(),
    });
    const lines = [metaLine, ...session.messages.map((m) => JSON.stringify(m))].join("\n") + "\n";
    fs.writeFileSync(tmp, lines, "utf8");
    if (fsync) {
      const fd = fs.openSync(tmp, "r+");
      try {
        fs.fsyncSync(fd);
      } finally {
        fs.closeSync(fd);
      }
    }
    fs.renameSync(tmp, p);
  } catch (e) {
    try {
      fs.unlinkSync(tmp);
    } catch {
      // 清理半成品失败不影响原始错误。
    }
    throw e;
  }
}
