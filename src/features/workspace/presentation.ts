import {
  messageText,
  type ConversationMessage,
  type ToolPart,
  type ToolStatus,
} from "@/features/pi-connection";

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

const statusVerbs: Record<ToolStatus, { active: string; done: string; failed: string }> = {
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

export function serializeConversation(
  title: string,
  messages: ConversationMessage[],
): string {
  const sections = messages.flatMap((message) => {
    const text = messageText(message).trim();
    if (!text) return [];
    const role = message.role === "user" ? "用户" : "Pi";
    return [`## ${role}\n\n${text}`];
  });
  return [`# ${title}`, ...sections].join("\n\n");
}
