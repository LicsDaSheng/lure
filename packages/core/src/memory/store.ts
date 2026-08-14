//! Memory 存储与 dream consolidation（对齐 crates/lure-core/src/memory/{store,consolidate}.rs）。

import fs from "node:fs";
import path from "node:path";
import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";
import type { CompletionRequest, LlmProvider } from "../provider/types.js";
import { defaultGenerationSettings } from "../provider/types.js";
import { stripThink } from "./strip.js";

/// history 单条最大字符数（应急上限）。
export const HISTORY_ENTRY_HARD_CAP = 64_000;
const TRUNCATION_MARKER = "\n... (truncated)";

export interface HistoryEntry {
  cursor: number;
  timestamp: string;
  content: string;
  sessionKey?: string;
}

/// 把当前记忆与未整合历史整合为新的长期记忆。
export interface DreamRunner {
  consolidate(currentMemory: string, entries: HistoryEntry[]): Promise<string>;
}

export interface ConsolidationOutcome {
  processed: number;
  newCursor: number;
}

export class MemoryStore {
  private readonly memoryFile: string;
  private readonly soulFile: string;
  private readonly userFile: string;
  private readonly historyFile: string;
  private readonly cursorFile: string;
  private readonly dreamCursorFile: string;
  private maxHistoryEntries?: number;

  constructor(workspace: string) {
    const memoryDir = path.join(workspace, "memory");
    fs.mkdirSync(memoryDir, { recursive: true });
    this.memoryFile = path.join(memoryDir, "MEMORY.md");
    this.soulFile = path.join(workspace, "SOUL.md");
    this.userFile = path.join(workspace, "USER.md");
    this.historyFile = path.join(memoryDir, "history.jsonl");
    this.cursorFile = path.join(memoryDir, ".cursor");
    this.dreamCursorFile = path.join(memoryDir, ".dream_cursor");
  }

  withMaxHistoryEntries(max: number): this {
    this.maxHistoryEntries = max;
    return this;
  }

  static readFile(p: string): string {
    try {
      return fs.readFileSync(p, "utf8");
    } catch {
      return "";
    }
  }

  readMemory(): string {
    return MemoryStore.readFile(this.memoryFile);
  }
  writeMemory(content: string): void {
    fs.writeFileSync(this.memoryFile, content);
  }
  readSoul(): string {
    return MemoryStore.readFile(this.soulFile);
  }
  writeSoul(content: string): void {
    fs.writeFileSync(this.soulFile, content);
  }
  readUser(): string {
    return MemoryStore.readFile(this.userFile);
  }
  writeUser(content: string): void {
    fs.writeFileSync(this.userFile, content);
  }

  /// 构建注入上下文的长期记忆块；无内容返回空串。
  getMemoryContext(): string {
    const longTerm = this.readMemory();
    return longTerm === "" ? "" : `## Long-term Memory\n${longTerm}`;
  }

  /// 追加一条 history 并返回其自增 cursor。
  appendHistory(entry: string, sessionKey?: string): number {
    const timestamp = formatTimestamp();
    let raw = entry.trimEnd();
    if ([...raw].length > HISTORY_ENTRY_HARD_CAP) {
      raw = `${[...raw].slice(0, HISTORY_ENTRY_HARD_CAP).join("")}${TRUNCATION_MARKER}`;
    }
    const content = stripThink(raw);

    const cursor = this.nextCursor();
    const record: HistoryEntry = { cursor, timestamp, content, sessionKey };
    const line = serializeEntry(record);

    fs.appendFileSync(this.historyFile, `${line}\n`);
    fs.writeFileSync(this.cursorFile, String(cursor));
    return cursor;
  }

  /// 返回 cursor > `sinceCursor` 的 history 条目。
  readUnprocessedHistory(sinceCursor: number): HistoryEntry[] {
    return this.validEntries().filter((e) => e.cursor > sinceCursor);
  }

  /// 返回可注入 prompt 的近程 history（按 session 过滤）。
  readRecentHistoryForPrompt(sinceCursor: number, sessionKey?: string): HistoryEntry[] {
    const entries = this.readUnprocessedHistory(sinceCursor);
    return sessionKey === undefined
      ? entries
      : entries.filter((e) => e.sessionKey === sessionKey);
  }

  getLastDreamCursor(): number {
    const n = Number.parseInt(MemoryStore.readFile(this.dreamCursorFile).trim(), 10);
    return Number.isNaN(n) ? 0 : n;
  }

  setLastDreamCursor(cursor: number): void {
    fs.writeFileSync(this.dreamCursorFile, String(cursor));
  }

  /// 用 runner 整合 dream cursor 之后的历史；无未整合历史返回 `undefined`。
  async consolidate<R extends DreamRunner>(runner: R): Promise<ConsolidationOutcome | undefined> {
    const since = this.getLastDreamCursor();
    const entries = this.readUnprocessedHistory(since);
    if (entries.length === 0) return undefined;

    const newMemory = await runner.consolidate(this.readMemory(), entries);
    this.writeMemory(newMemory);

    const newCursor = Math.max(since, ...entries.map((e) => e.cursor));
    this.setLastDreamCursor(newCursor);

    return { processed: entries.length, newCursor };
  }

  shouldConsolidate(minEntries: number): boolean {
    return this.readUnprocessedHistory(this.getLastDreamCursor()).length >= minEntries;
  }

  private nextCursor(): number {
    const counter = Number.parseInt(MemoryStore.readFile(this.cursorFile).trim(), 10) || 0;
    const maxEntry = this.validEntries().reduce((max, e) => Math.max(max, e.cursor), 0);
    return Math.max(counter, maxEntry) + 1;
  }

  private validEntries(): HistoryEntry[] {
    return MemoryStore.readFile(this.historyFile)
      .split("\n")
      .filter((line) => line.trim() !== "")
      .map((line) => {
        try {
          return JSON.parse(line) as Json;
        } catch {
          return undefined;
        }
      })
      .filter((v): v is Json => v !== undefined)
      .map(parseEntry)
      .filter((e): e is HistoryEntry => e !== undefined);
  }
}

/// 真实 LLM 驱动的 dream consolidation runner。
export class ProviderDreamRunner implements DreamRunner {
  constructor(private readonly provider: LlmProvider) {}

  async consolidate(currentMemory: string, entries: HistoryEntry[]): Promise<string> {
    const userText = entries.map((e) => e.content).join("\n\n");
    const prompt =
      `你是一个记忆整合助手。下面是当前的长期记忆：\n\n${currentMemory}\n\n` +
      `下面是最近的对话历史条目，请提取其中有价值的信息并更新上面的长期记忆。` +
      `直接返回更新后的完整 MEMORY.md 内容，不要加额外解释。\n\n${userText}`;

    const request: CompletionRequest = {
      model: "",
      messages: [{ role: "user", content: prompt }],
      settings: defaultGenerationSettings(),
      tools: [],
    };

    try {
      const resp = await this.provider.complete(request);
      const text = resp.content ?? "";
      return text.trim() === "" ? currentMemory : text;
    } catch {
      return currentMemory;
    }
  }
}

function serializeEntry(e: HistoryEntry): string {
  const obj: Record<string, Json> = { cursor: e.cursor, timestamp: e.timestamp, content: e.content };
  if (e.sessionKey !== undefined) obj["session_key"] = e.sessionKey;
  return JSON.stringify(obj);
}

function parseEntry(value: Json): HistoryEntry | undefined {
  const obj = asObject(value);
  if (obj === undefined) return undefined;
  const cursor = obj["cursor"];
  if (typeof cursor !== "number" || !Number.isInteger(cursor) || cursor < 0) return undefined;
  const timestamp = asString(obj["timestamp"]);
  if (timestamp === undefined) return undefined;
  const content = asString(obj["content"]);
  if (content === undefined) return undefined;
  const sk = obj["session_key"];
  let sessionKey: string | undefined;
  if (sk === undefined || sk === null) sessionKey = undefined;
  else if (typeof sk === "string") sessionKey = sk;
  else return undefined;
  return { cursor, timestamp, content, sessionKey };
}

function formatTimestamp(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
