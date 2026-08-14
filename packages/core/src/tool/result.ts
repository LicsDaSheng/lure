//! 工具结果与结果截断（对齐 crates/lure-core/src/tool/result.rs）。

export class ToolResult {
  constructor(
    readonly content: string,
    readonly isError: boolean,
  ) {}

  static ok(content: string): ToolResult {
    return new ToolResult(content, false);
  }

  static error(content: string): ToolResult {
    return new ToolResult(content, true);
  }
}

const TRUNCATION_MARKER = "\n… (truncated)";

/// 若文本字符数超过 `maxChars`，截断并追加省略标记；`maxChars == 0` 不截断。
export function truncateResult(text: string, maxChars: number): string {
  if (maxChars === 0 || [...text].length <= maxChars) return text;
  return `${[...text].slice(0, maxChars).join("")}${TRUNCATION_MARKER}`;
}
