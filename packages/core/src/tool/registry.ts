//! 工具契约与注册表（对齐 crates/lure-core/src/tool/registry.rs）。

import { err, ok, type Result } from "neverthrow";
import type { Json } from "../provider/types.js";
import { validateValue } from "./schema.js";
import type { ToolResult } from "./result.js";

export interface Tool {
  name(): string;
  description(): string;
  parameters(): Json;
  readOnly(): boolean;
  execute(args: Json): ToolResult;
}

export class ToolError extends Error {
  constructor(
    readonly kind: "unknown_tool" | "invalid_args",
    message: string,
    readonly name: string,
    readonly suggestion?: string,
    readonly errors?: string[],
  ) {
    super(message);
    this.name = "ToolError";
  }
}

export class ToolRegistry {
  private readonly tools = new Map<string, Tool>();
  private readonly order: string[] = [];

  register(tool: Tool): void {
    const name = tool.name();
    if (!this.tools.has(name)) this.order.push(name);
    this.tools.set(name, tool);
  }

  names(): string[] {
    return [...this.order];
  }

  contains(name: string): boolean {
    return this.tools.has(name);
  }

  /// 产出 OpenAI function-calling 定义（注册顺序）。
  getDefinitions(): Json[] {
    return this.order.map((name) => {
      const tool = this.tools.get(name)!;
      return {
        type: "function",
        function: {
          name: tool.name(),
          description: tool.description(),
          parameters: tool.parameters(),
        },
      };
    });
  }

  /// 按名校验参数并派发执行。
  execute(name: string, args: Json): Result<ToolResult, ToolError> {
    const tool = this.tools.get(name);
    if (tool === undefined) {
      return err(this.unknownTool(name));
    }
    const errors = validateValue(args, tool.parameters(), "");
    if (errors.length > 0) {
      return err(new ToolError("invalid_args", `工具 '${name}' 参数无效: ${errors.join("; ")}`, name, undefined, errors));
    }
    return ok(tool.execute(args));
  }

  private unknownTool(name: string): ToolError {
    const suggestion = this.suggest(name);
    const message =
      suggestion !== undefined ? `未知工具 '${name}'，是否想调用 '${suggestion}'?` : `未知工具 '${name}'`;
    return new ToolError("unknown_tool", message, name, suggestion);
  }

  private suggest(name: string): string | undefined {
    const target = normalize(name);
    return this.order.find((r) => normalize(r) === target);
  }
}

function normalize(value: string): string {
  return value
    .split("")
    .filter((c) => c !== "_" && c !== "-")
    .join("")
    .toLowerCase();
}
