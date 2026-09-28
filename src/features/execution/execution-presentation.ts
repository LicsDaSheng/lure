import type { ToolPart, ToolStatus, TurnPhase } from "@/lib/pi-rpc/types";

/** 渲染工具执行所需的最小数据，由工具调用部件直接派生。 */
export type ToolView = {
  id: string;
  name: string;
  status: ToolStatus;
  input: string;
  output: string;
  truncatedLines: number | null;
};

/** 把 Pi 工具 part 一次转换为 assistant-ui 渲染器直接消费的展示数据。 */
export function toolPartToView(tool: ToolPart): ToolView {
  return {
    id: tool.toolCallId,
    name: tool.name,
    status: tool.status,
    input: tool.input,
    output: tool.output,
    truncatedLines: tool.truncatedLines,
  };
}

const statusVerbs: Record<
  ToolStatus,
  { active: string; done: string; failed: string }
> = {
  running: { active: "正在", done: "正在", failed: "正在" },
  completed: { active: "已", done: "已", failed: "已" },
  error: { active: "无法", done: "无法", failed: "无法" },
};

export function formatToolSummary(tool: ToolView): string {
  const target = extractToolTarget(tool.input);
  const prefix = statusVerbs[tool.status];
  switch (tool.name) {
    case "read":
      return `${tool.status === "error" ? prefix.failed : tool.status === "running" ? prefix.active : prefix.done}${tool.status === "error" ? "读取" : "读取"}${target ? ` ${target}` : "项目内容"}`;
    case "edit":
      return `${tool.status === "error" ? "无法修改" : tool.status === "running" ? "正在修改" : "已修改"}${target ? ` ${target}` : "文件"}`;
    case "write":
      return `${tool.status === "error" ? "无法创建" : tool.status === "running" ? "正在创建" : "已创建"}${target ? ` ${target}` : "文件"}`;
    case "bash":
      return tool.status === "error"
        ? "命令执行失败"
        : tool.status === "running"
          ? "正在执行项目命令"
          : "项目命令执行完成";
    case "web_search":
    case "fetch_content":
      return tool.status === "error"
        ? "无法获取外部资料"
        : tool.status === "running"
          ? "正在获取外部资料"
          : "已获取外部资料";
    default:
      return tool.status === "error"
        ? "工具步骤执行失败"
        : tool.status === "running"
          ? "Pi 正在使用工具处理当前步骤"
          : "工具步骤已完成";
  }
}

export function extractToolTarget(input: string): string | null {
  try {
    const parsed = JSON.parse(input) as Record<string, unknown>;
    for (const key of ["path", "file_path", "filePath", "url"]) {
      if (typeof parsed[key] === "string" && parsed[key]) return parsed[key];
    }
  } catch {
    return null;
  }
  return null;
}

export function getToolOutputLineCount(tool: ToolView): number {
  const visible = tool.output ? tool.output.split("\n").length : 0;
  return visible + (tool.truncatedLines ?? 0);
}

const SECOND_MS = 1000;

/** 把运行耗时描述成简短的中文时长。 */
export function formatDuration(durationMs: number): string {
  const seconds = Math.round(durationMs / SECOND_MS);
  if (seconds < 1) return "不到 1 秒";
  if (seconds < 60) return `${seconds} 秒`;

  const minutes = Math.floor(seconds / 60);
  const rest = seconds % 60;
  return rest === 0 ? `${minutes} 分` : `${minutes} 分 ${rest} 秒`;
}

/**
 * 执行过程折叠区的触发器文案。
 *
 * 同时说明状态和点击后的行为：缺少可靠耗时的历史记录只显示工具数量，
 * 不用猜测出来的时长误导用户。
 */
export function executionProcessLabel({
  durationMs,
  expanded,
  phase,
  toolCount,
}: {
  durationMs: number | null;
  expanded: boolean;
  phase: TurnPhase;
  toolCount: number;
}): string {
  if (phase === "running") return expanded ? "执行中" : "执行中 · 展开执行过程";

  const prefix = (() => {
    switch (phase) {
      case "aborted":
        return "已停止";
      case "truncated":
        return "响应被截断";
      case "error":
        return "执行未完成";
      default:
        if (durationMs !== null) return `用时 ${formatDuration(durationMs)}`;
        return toolCount > 0 ? `${toolCount} 个工具调用` : "执行过程";
    }
  })();
  const action = expanded
    ? "收起执行过程"
    : phase === "settled"
      ? "展开执行过程"
      : phase === "aborted"
        ? "查看已完成过程"
        : "查看过程";
  return `${prefix} · ${action}`;
}
