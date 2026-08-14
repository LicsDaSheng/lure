import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { validateValue } from "./schema.js";
import { truncateResult, ToolResult } from "./result.js";
import { ToolRegistry } from "./registry.js";
import { findMatch, ReadFileTool, WriteFileTool, EditFileTool } from "./file.js";
import { ExecPolicy, splitTopLevelSegments } from "./shell.js";

function tempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-tool-"));
}

function echoTool() {
  return {
    name: () => "echo",
    description: () => "echo",
    parameters: () => ({ type: "object", properties: { text: { type: "string" } }, required: ["text"] }),
    readOnly: () => true,
    execute: (args: unknown) => ToolResult.ok(`echo: ${(args as Record<string, unknown>)["text"]}`),
  };
}

describe("tool schema validateValue", () => {
  it("validates type/required/enum/min", () => {
    expect(validateValue(5, { type: "integer" }, "")).toEqual([]);
    expect(validateValue("x", { type: "integer" }, "")).toEqual(["parameter should be integer"]);
    expect(validateValue({}, { type: "object", required: ["path"] }, "")).toEqual(["missing required path"]);
    expect(validateValue(2, { type: "integer", enum: [1, 3] }, "")).toEqual(["parameter must be one of the allowed values"]);
    expect(validateValue(2, { type: "integer", minimum: 5 }, "")).toEqual(["parameter must be >= 5"]);
  });
});

describe("tool result truncate", () => {
  it("truncates long text and keeps short", () => {
    expect(truncateResult("hi", 10)).toBe("hi");
    expect(truncateResult("hello world", 5)).toBe("hello\n… (truncated)");
  });
});

describe("ToolRegistry", () => {
  it("registers, lists names, dispatches, suggests", () => {
    const reg = new ToolRegistry();
    reg.register(echoTool());
    expect(reg.names()).toEqual(["echo"]);
    expect(reg.contains("echo")).toBe(true);

    const defs = reg.getDefinitions();
    expect(defs[0]).toMatchObject({ type: "function", function: { name: "echo" } });

    const r = reg.execute("echo", { text: "hi" });
    expect(r.isOk()).toBe(true);
    if (r.isOk()) expect(r.value.content).toBe("echo: hi");

    const missing = reg.execute("echo_", { text: "x" });
    expect(missing.isErr()).toBe(true);
    if (missing.isErr()) expect(missing.error.suggestion).toBe("echo");

    const invalid = reg.execute("echo", {});
    expect(invalid.isErr()).toBe(true);
  });
});

describe("file tools", () => {
  it("read/write/edit within workspace", () => {
    const ws = tempDir();
    const reg = new ToolRegistry();
    reg.register(new ReadFileTool(ws));
    reg.register(new WriteFileTool(ws));
    reg.register(new EditFileTool(ws));

    expect(reg.execute("write_file", { path: "a.txt", content: "hello" }).isOk()).toBe(true);
    const read = reg.execute("read_file", { path: "a.txt" });
    expect(read.isOk()).toBe(true);
    if (read.isOk()) expect(read.value.content).toBe("hello");

    const edit = reg.execute("edit_file", { path: "a.txt", old_text: "hello", new_text: "world" });
    expect(edit.isOk()).toBe(true);
    expect(fs.readFileSync(path.join(ws, "a.txt"), "utf8")).toBe("world");
  });

  it("rejects path outside workspace", () => {
    const ws = tempDir();
    const reg = new ToolRegistry();
    reg.register(new ReadFileTool(ws));
    const r = reg.execute("read_file", { path: "../outside.txt" });
    expect(r.isOk()).toBe(true);
    if (r.isOk()) expect(r.value.isError).toBe(true);
  });
});

describe("findMatch", () => {
  it("exact and line-trim matching", () => {
    expect(findMatch("a\nb", "b")).toEqual(["b", 1]);
    expect(findMatch("  hello\n", "hello")).toEqual(["hello", 1]);
    expect(findMatch("  hello\n  world\n", "hello\nworld")).toEqual(["  hello\n  world", 1]);
    expect(findMatch("xyz", "nope")).toEqual([undefined, 0]);
  });
});

describe("ExecPolicy", () => {
  it("allowlist passes when all segments allowed", () => {
    const policy = ExecPolicy.new(["^ls\\b", "^cat\\b"], []);
    expect(policy.guardCommand("ls -la")).toBeUndefined();
    expect(policy.guardCommand("ls && cat a")).toBeUndefined();
    expect(policy.guardCommand("rm -rf /")).toContain("拦截");
  });

  it("splits top-level segments but keeps fd redirection &", () => {
    expect(splitTopLevelSegments("ls 2>&1 && cat")).toEqual(["ls 2>&1", "cat"]);
    expect(splitTopLevelSegments("a; b | c")).toEqual(["a", "b", "c"]);
  });
});
