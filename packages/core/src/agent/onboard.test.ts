import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { ensureWorkspace } from "./onboard.js";

function tempRoot(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-onboard-"));
}

describe("ensureWorkspace", () => {
  it("creates dirs, config and templates", () => {
    const root = tempRoot();
    const summary = ensureWorkspace(root);

    expect(summary.root).toBe(root);
    expect(summary.workspace).toBe(path.join(root, "workspace"));
    expect(fs.existsSync(path.join(root, "config.json"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "sessions"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "SOUL.md"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "USER.md"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "AGENTS.md"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "HEARTBEAT.md"))).toBe(true);
    expect(fs.existsSync(path.join(root, "workspace", "memory", "MEMORY.md"))).toBe(true);
  });

  it("does not overwrite existing user files", () => {
    const root = tempRoot();
    ensureWorkspace(root);

    const soulPath = path.join(root, "workspace", "SOUL.md");
    fs.writeFileSync(soulPath, "custom soul");
    const configPath = path.join(root, "config.json");
    fs.writeFileSync(configPath, '{"custom":true}');

    ensureWorkspace(root);

    expect(fs.readFileSync(soulPath, "utf8")).toBe("custom soul");
    expect(fs.readFileSync(configPath, "utf8")).toBe('{"custom":true}');
  });
});
