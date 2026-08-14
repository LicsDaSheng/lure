//! shell 执行策略与工具（对齐 crates/lure-core/src/tool/shell.rs）。

import { execFileSync } from "node:child_process";
import type { Json } from "../provider/types.js";
import { asObject, asString } from "../provider/json.js";
import { truncateResult, ToolResult } from "./result.js";
import type { Tool } from "./registry.js";

const BUILTIN_DENY = [/\brm\s+-rf\b/, /\bmkfs\b/, /\bdd\s+if=/, />\s*\/dev\/sd/];
const MAX_EXEC_RESULT_CHARS = 16_000;

export class ExecPolicy {
  constructor(
    private readonly allow: RegExp[],
    private readonly deny: RegExp[],
  ) {}

  /// 以 allow/额外 deny 模式构造（额外 deny 追加到内建之后）；正则非法抛错。
  static new(allowPatterns: string[], extraDenyPatterns: string[]): ExecPolicy {
    const allow = allowPatterns.map((p) => new RegExp(p));
    const deny = [...BUILTIN_DENY, ...extraDenyPatterns.map((p) => new RegExp(p))];
    return new ExecPolicy(allow, deny);
  }

  /// 校验命令；`undefined` 表示放行，字符串表示拦截原因。
  guardCommand(command: string): string | undefined {
    const segments = splitTopLevelSegments(command);

    if (this.allow.length > 0) {
      const allAllowed = segments.every((seg) => this.allow.some((re) => re.test(seg)));
      if (allAllowed) return undefined;
    }

    if (this.deny.some((re) => re.test(command))) {
      return `命令被 deny pattern filter 拦截: ${command}`;
    }

    if (this.allow.length > 0) {
      return `命令未通过 allowlist: ${command}`;
    }
    return undefined;
  }
}

/// 按顶层 shell 操作符切分命令，保留 fd 重定向中的 `&`。
export function splitTopLevelSegments(command: string): string[] {
  const segments: string[] = [];
  let current = "";
  let inSingle = false;
  let inDouble = false;
  let i = 0;

  while (i < command.length) {
    const c = command[i]!;

    if (inSingle) {
      current += c;
      if (c === "'") inSingle = false;
      i += 1;
      continue;
    }
    if (inDouble) {
      current += c;
      if (c === '"') inDouble = false;
      i += 1;
      continue;
    }

    if (c === "'") {
      inSingle = true;
      current += c;
      i += 1;
    } else if (c === '"') {
      inDouble = true;
      current += c;
      i += 1;
    } else if (c === ";") {
      segments.push(current);
      current = "";
      i += 1;
    } else if (c === "|") {
      segments.push(current);
      current = "";
      i += command[i + 1] === "|" ? 2 : 1;
    } else if (c === "&") {
      if (command[i + 1] === "&") {
        segments.push(current);
        current = "";
        i += 2;
      } else if (isFdRedirection(command, i)) {
        current += c;
        i += 1;
      } else {
        segments.push(current);
        current = "";
        i += 1;
      }
    } else {
      current += c;
      i += 1;
    }
  }
  segments.push(current);
  return segments.map((s) => s.trim());
}

function isFdRedirection(command: string, index: number): boolean {
  const prev = command.slice(0, index).split("").reverse().find((c) => !/\s/.test(c));
  const next = command.slice(index + 1).split("").find((c) => !/\s/.test(c));
  return prev === ">" || next === ">";
}

export class ExecTool implements Tool {
  constructor(
    private readonly policy: ExecPolicy,
    private readonly workspace: string,
  ) {}

  name(): string {
    return "exec";
  }
  description(): string {
    return "在 workspace 内执行 shell 命令，受 allow/deny 策略约束。";
  }
  parameters(): Json {
    return {
      type: "object",
      properties: { command: { type: "string", description: "要执行的 shell 命令" } },
      required: ["command"],
    };
  }
  execute(args: Json): ToolResult {
    const command = asString(asObject(args)?.["command"]);
    if (command === undefined) return ToolResult.error("缺少 command 参数");

    const reason = this.policy.guardCommand(command);
    if (reason !== undefined) return ToolResult.error(reason);

    try {
      const stdout = execFileSync("sh", ["-c", command], {
        cwd: this.workspace,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
      });
      return ToolResult.ok(truncateResult(stdout, MAX_EXEC_RESULT_CHARS));
    } catch (e) {
      const err = e as { stdout?: string; stderr?: string; message?: string };
      const combined = [err.stdout, err.stderr].filter(Boolean).join("");
      const content = truncateResult(combined || err.message || String(e), MAX_EXEC_RESULT_CHARS);
      return ToolResult.error(`命令退出码非零: ${content}`);
    }
  }
}
