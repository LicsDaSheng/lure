import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { sessionKeyForChannel, UNIFIED_SESSION_KEY } from "./keys.js";
import { FILE_MAX_MESSAGES, Session } from "./model.js";
import { SessionManager } from "./store.js";
import { goalStateRuntimeLines, goalStateWsBlob, sustainedGoalActive } from "./goal_state.js";
import { SessionLocks } from "./lock.js";

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-session-"));
}

describe("session keys", () => {
  it("formats channel:chat_id or unified", () => {
    expect(sessionKeyForChannel("webui", "c1", false)).toBe("webui:c1");
    expect(sessionKeyForChannel("webui", "c1", true)).toBe(UNIFIED_SESSION_KEY);
  });
});

describe("session model", () => {
  it("adds message with timestamp and clamps offset", () => {
    const s = new Session("k");
    s.addMessage("user", "hi");
    s.addMessageWith("assistant", "yo", { reasoning_content: "think" });
    expect(s.messages).toHaveLength(2);
    expect(s.messages[1]).toMatchObject({ role: "assistant", content: "yo", reasoning_content: "think" });
    expect(typeof s.messages[0]?.["timestamp"]).toBe("string");
  });

  it("getHistory slices unconsolidated tail", () => {
    const s = new Session("k");
    for (let i = 0; i < 5; i++) s.addMessage("user", `m${i}`);
    expect(s.getHistory(0)).toHaveLength(5);
    expect(s.getHistory(2)).toEqual([s.messages[3], s.messages[4]]);
    expect(s.getHistory(2).map((m) => (m as Record<string, unknown>)["content"])).toEqual(["m3", "m4"]);
  });

  it("clamps invalid last_consolidated to 0", () => {
    const s = Session.fromLoaded("k", [{ role: "user", content: "x" }], {}, "now", "now", 99);
    expect(s.lastConsolidatedOffset()).toBe(0);
    const s2 = Session.fromLoaded("k", [{ role: "user", content: "x" }], {}, "now", "now", -1);
    expect(s2.lastConsolidatedOffset()).toBe(0);
    const s3 = Session.fromLoaded("k", [{ role: "user", content: "x" }], {}, "now", "now", 1);
    expect(s3.lastConsolidatedOffset()).toBe(1);
  });
});

describe("SessionManager", () => {
  it("storage key round-trips via base64url", () => {
    for (const key of ["webui:c1", "telegram:123", "unified:default"]) {
      const encoded = SessionManager.storageKey(key);
      expect(encoded).not.toContain("=");
      expect(SessionManager.decodeStorageKey(encoded)).toBe(key);
    }
  });

  it("persists and reloads a session", () => {
    const dir = tempDir();
    let mgr = SessionManager.forWorkspace(dir);
    const s = mgr.getOrCreate("webui:c1");
    s.addMessage("user", "hi");
    s.addMessage("assistant", "yo");
    mgr.save("webui:c1", false);

    // 新建 manager 从磁盘加载。
    mgr = SessionManager.forWorkspace(dir);
    const loaded = mgr.getOrCreate("webui:c1");
    expect(loaded.messages).toHaveLength(2);
    expect(loaded.messages[0]).toMatchObject({ role: "user", content: "hi" });
  });

  it("evicts least-recently-used beyond cache cap", () => {
    const dir = tempDir();
    const mgr = SessionManager.forWorkspace(dir);
    mgr.setMaxCached(2);
    mgr.getOrCreate("a");
    mgr.getOrCreate("b");
    mgr.getOrCreate("c");
    expect(mgr.cacheLen()).toBe(2);
    expect(mgr.cacheKeys()).toEqual(["b", "c"]);
  });

  it("lists stored keys", () => {
    const dir = tempDir();
    const mgr = SessionManager.forWorkspace(dir);
    mgr.getOrCreate("webui:c1").addMessage("user", "hi");
    mgr.save("webui:c1", false);
    mgr.getOrCreate("telegram:42").addMessage("user", "hi");
    mgr.save("telegram:42", false);
    expect(mgr.listStoredKeys()).toEqual(["telegram:42", "webui:c1"]);
  });
});

describe("goal state", () => {
  it("detects active sustained goal and emits runtime lines", () => {
    const metadata = { goal_state: { status: "active", objective: "ship it", ui_summary: "ship" } };
    expect(sustainedGoalActive(metadata)).toBe(true);
    expect(goalStateRuntimeLines(metadata)).toEqual(["Goal (active):", "ship it", "Summary: ship"]);
    expect(goalStateWsBlob(metadata)).toEqual({ active: true, ui_summary: "ship", objective: "ship it" });
  });

  it("inactive goal yields inactive blob", () => {
    expect(sustainedGoalActive({ goal_state: { status: "done" } })).toBe(false);
    expect(goalStateWsBlob({ goal_state: { status: "done" } })).toEqual({ active: false });
  });
});

describe("session lock", () => {
  it("tryLock is exclusive per key, independent across keys", () => {
    const locks = new SessionLocks();
    const g = locks.tryLock("k");
    expect(g?.key).toBe("k");
    expect(locks.tryLock("k")).toBeUndefined();
    expect(locks.tryLock("other")?.key).toBe("other");
    g?.release();
    expect(locks.tryLock("k")?.key).toBe("k");
  });
});
