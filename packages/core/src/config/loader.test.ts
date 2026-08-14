import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { defaultConfig, DEFAULT_MODEL } from "@lure/schema";
import { loadConfig, saveConfig } from "./loader.js";

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-test-"));
}

function unwrap<T, E>(r: { isErr(): boolean; value: T; error: E }): T {
  if (r.isErr()) throw r.error;
  return r.value;
}

describe("load_config", () => {
  it("missing file uses defaults", () => {
    const config = unwrap(loadConfig(path.join(tempDir(), "missing.json")));
    expect(config.agents.defaults.model).not.toBe("");
    expect(config.agents.defaults.model).toBe(DEFAULT_MODEL);
  });

  it("invalid json fails fast", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");
    fs.writeFileSync(p, "{broken json");

    const r = loadConfig(p);
    expect(r.isErr()).toBe(true);
    if (r.isErr()) {
      expect(r.error.kind).toBe("parse");
      expect(r.error.message).toContain("加载配置失败");
    }
  });

  it("type mismatch fails fast", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");
    fs.writeFileSync(p, `{"agents":{"defaults":{"maxTokens":-1}}}`);

    const r = loadConfig(p);
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect(r.error.kind).toBe("parse");
  });

  it("applies migration and ignores legacy", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");
    fs.writeFileSync(p, `{"agents":{"defaults":{"maxMessages":25}}}`);

    const config = unwrap(loadConfig(p));
    expect(config.agents.defaults.model).toBe(DEFAULT_MODEL);
  });

  it("rejects invalid preset", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");
    fs.writeFileSync(p, `{"agents":{"defaults":{"modelPreset":"nope"}}}`);

    const r = loadConfig(p);
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect(r.error.message).toContain("配置校验失败");
  });
});

describe("save_config", () => {
  it("round trips", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");

    expect(saveConfig(defaultConfig(), p).isOk()).toBe(true);
    const loaded = unwrap(loadConfig(p));

    expect(loaded).toEqual(defaultConfig());
    expect(loaded.agents.defaults.model).not.toBe("");
  });

  it("emits camelCase indented json", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");

    expect(saveConfig(defaultConfig(), p).isOk()).toBe(true);
    const text = fs.readFileSync(p, "utf8");

    expect(text).toContain('"maxTokens"');
    expect(text).not.toContain("max_tokens");
    expect(text).toContain('\n  "');
  });

  it("creates missing parent dirs", () => {
    const dir = tempDir();
    const p = path.join(dir, "nested", "instance", "config.json");

    expect(saveConfig(defaultConfig(), p).isOk()).toBe(true);
    expect(fs.existsSync(p)).toBe(true);
  });

  it("preserves existing file mode", () => {
    const dir = tempDir();
    const p = path.join(dir, "config.json");
    fs.writeFileSync(p, "{}");
    fs.chmodSync(p, 0o600);

    expect(saveConfig(defaultConfig(), p).isOk()).toBe(true);
    expect(fs.statSync(p).mode & 0o777).toBe(0o600);
  });
});
