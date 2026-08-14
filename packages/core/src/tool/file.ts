//! 受 workspace 约束的文件工具（对齐 crates/lure-core/src/tool/file.rs）。

import fs from "node:fs";
import path from "node:path";
import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";
import { resolveInWorkspace } from "../security/workspace.js";
import { truncateResult, ToolResult } from "./result.js";
import type { Tool } from "./registry.js";

const MAX_READ_CHARS = 50_000;

function resolveOrError(inputPath: string, workspace: string): string | ToolResult {
  try {
    return resolveInWorkspace(inputPath, workspace);
  } catch (e) {
    return ToolResult.error(String(e));
  }
}

export class ReadFileTool implements Tool {
  constructor(private readonly workspace: string) {}

  name(): string {
    return "read_file";
  }
  description(): string {
    return "读取 workspace 内的文本文件。";
  }
  parameters(): Json {
    return {
      type: "object",
      properties: { path: { type: "string", description: "相对 workspace 的路径" } },
      required: ["path"],
    };
  }
  readOnly(): boolean {
    return true;
  }
  execute(args: Json): ToolResult {
    const p = asString(asObject(args)?.["path"]);
    if (p === undefined) return ToolResult.error("缺少 path 参数");
    const resolved = resolveOrError(p, this.workspace);
    if (resolved instanceof ToolResult) return resolved;
    try {
      return ToolResult.ok(truncateResult(fs.readFileSync(resolved, "utf8"), MAX_READ_CHARS));
    } catch (e) {
      return ToolResult.error(`读取失败 ${resolved}: ${e}`);
    }
  }
}

export class WriteFileTool implements Tool {
  constructor(private readonly workspace: string) {}

  name(): string {
    return "write_file";
  }
  description(): string {
    return "写入 workspace 内的文本文件（必要时创建父目录）。";
  }
  parameters(): Json {
    return {
      type: "object",
      properties: {
        path: { type: "string", description: "相对 workspace 的路径" },
        content: { type: "string", description: "写入内容" },
      },
      required: ["path", "content"],
    };
  }
  execute(args: Json): ToolResult {
    const obj = asObject(args);
    const p = asString(obj?.["path"]);
    const content = asString(obj?.["content"]);
    if (p === undefined) return ToolResult.error("缺少 path 参数");
    if (content === undefined) return ToolResult.error("缺少 content 参数");
    const resolved = resolveOrError(p, this.workspace);
    if (resolved instanceof ToolResult) return resolved;
    try {
      fs.mkdirSync(path.dirname(resolved), { recursive: true });
      fs.writeFileSync(resolved, content, "utf8");
      return ToolResult.ok(`已写入 ${content.length} 字节到 ${p}`);
    } catch (e) {
      return ToolResult.error(`写入失败 ${resolved}: ${e}`);
    }
  }
}

/// 在 `content` 中定位 `old_text`，返回 `[实际匹配文本, 匹配数]`。
export function findMatch(content: string, oldText: string): [string | undefined, number] {
  if (oldText === "") return ["", 1];

  const exact = content.split(oldText).length - 1;
  if (exact > 0) return [oldText, exact];

  const strippedOld = oldText.split("\n").map((l) => l.trim());
  if (strippedOld.length === 0) return [undefined, 0];
  const contentLines = content.split("\n");
  const window = strippedOld.length;
  if (contentLines.length < window) return [undefined, 0];

  let count = 0;
  let firstMatch: string | undefined;
  for (let start = 0; start <= contentLines.length - window; start++) {
    const comparable = contentLines.slice(start, start + window).map((l) => l.trim());
    if (comparable.every((l, i) => l === strippedOld[i])) {
      count += 1;
      if (firstMatch === undefined) firstMatch = contentLines.slice(start, start + window).join("\n");
    }
  }
  return count === 0 ? [undefined, 0] : [firstMatch, count];
}

export class EditFileTool implements Tool {
  constructor(private readonly workspace: string) {}

  name(): string {
    return "edit_file";
  }
  description(): string {
    return "编辑 workspace 内文本文件：定位 old_text 并替换为 new_text。";
  }
  parameters(): Json {
    return {
      type: "object",
      properties: {
        path: { type: "string", description: "相对 workspace 的路径" },
        old_text: { type: "string", description: "要替换的原文" },
        new_text: { type: "string", description: "替换后的新文本" },
        replace_all: { type: "boolean", description: "替换全部匹配（默认否）" },
      },
      required: ["path", "old_text", "new_text"],
    };
  }
  execute(args: Json): ToolResult {
    const obj = asObject(args);
    const p = asString(obj?.["path"]);
    const oldText = asString(obj?.["old_text"]);
    const newText = asString(obj?.["new_text"]);
    const replaceAll = obj?.["replace_all"] === true;
    if (p === undefined) return ToolResult.error("缺少 path 参数");
    if (oldText === undefined) return ToolResult.error("Error editing file: Unknown old_text");
    if (newText === undefined) return ToolResult.error("Error editing file: Unknown new_text");

    const resolved = resolveOrError(p, this.workspace);
    if (resolved instanceof ToolResult) return resolved;
    let raw: string;
    try {
      raw = fs.readFileSync(resolved, "utf8");
    } catch (e) {
      return ToolResult.error(`读取失败 ${resolved}: ${e}`);
    }

    const hadCrlf = raw.includes("\r\n");
    const normalized = raw.replace(/\r\n/g, "\n");

    const [matched, count] = findMatch(normalized, oldText);
    if (matched === undefined || count === 0) {
      return ToolResult.error("Error editing file: old_text not found");
    }
    if (count > 1 && !replaceAll) {
      return ToolResult.error(
        `Warning: old_text appears ${count} times; pass replace_all=true or add more context`,
      );
    }

    const edited = replaceAll
      ? normalized.split(matched).join(newText)
      : normalized.replace(matched, newText);
    const output = hadCrlf ? edited.replace(/\n/g, "\r\n") : edited;

    try {
      fs.writeFileSync(resolved, output, "utf8");
      return ToolResult.ok(`Successfully edited ${p}`);
    } catch (e) {
      return ToolResult.error(`写入失败 ${resolved}: ${e}`);
    }
  }
}
