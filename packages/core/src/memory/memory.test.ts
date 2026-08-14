import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { stripThink } from "./strip.js";
import { MemoryStore, type DreamRunner } from "./store.js";

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-memory-"));
}

describe("stripThink", () => {
  it("removes think blocks, channel markers, malformed tags", () => {
    expect(stripThink("a<think>x</think>b")).toBe("ab");
    expect(stripThink("<thinking>secret</thinking>done")).toBe("done");
    expect(stripThink("<|channel|>hello")).toBe("hello");
    expect(stripThink("<think hello")).toBe(" hello");
    expect(stripThink("<think>unclosed")).toBe("");
  });
});

describe("MemoryStore", () => {
  it("reads/writes memory files and builds context", () => {
    const ws = tempDir();
    const store = new MemoryStore(ws);
    store.writeMemory("# Memory\n\nfact");
    expect(store.readMemory()).toContain("fact");
    expect(store.getMemoryContext()).toBe("## Long-term Memory\n# Memory\n\nfact");
    expect(store.getMemoryContext()).toBe(store.getMemoryContext());
  });

  it("appends history with incrementing cursor and strips think", () => {
    const ws = tempDir();
    const store = new MemoryStore(ws);
    const c1 = store.appendHistory("hello <think>x</think>", "s1");
    const c2 = store.appendHistory("world", "s2");
    expect(c2).toBe(c1 + 1);

    const all = store.readUnprocessedHistory(0);
    expect(all).toHaveLength(2);
    expect(all[0]?.content).toBe("hello");
    expect(all[0]?.sessionKey).toBe("s1");
  });

  it("filters recent history by session", () => {
    const ws = tempDir();
    const store = new MemoryStore(ws);
    store.appendHistory("a", "s1");
    store.appendHistory("b", "s2");
    expect(store.readRecentHistoryForPrompt(0, "s1").map((e) => e.content)).toEqual(["a"]);
  });

  it("consolidates with a fake runner", async () => {
    const ws = tempDir();
    const store = new MemoryStore(ws);
    store.appendHistory("learned fact");
    store.appendHistory("another fact");

    const runner: DreamRunner = {
      consolidate: async (memory, entries) => `# Memory\n\n${entries.length} entries`,
    };
    const outcome = await store.consolidate(runner);

    expect(outcome).toBeDefined();
    expect(outcome?.processed).toBe(2);
    expect(store.readMemory()).toContain("2 entries");
    expect(store.getLastDreamCursor()).toBe(outcome?.newCursor);
  });
});
