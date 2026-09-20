import type {
  ConversationMessage,
  ToolRun,
} from "@/features/pi-connection/reducer";

export type ResultDescriptor = {
  type: "file" | "diff" | "log" | "link";
  title: string;
  description: string;
  actionLabel: string;
  content: string;
};

const statusVerbs: Record<ToolRun["status"], { active: string; done: string; failed: string }> = {
  running: { active: "正在", done: "正在", failed: "正在" },
  completed: { active: "已", done: "已", failed: "已" },
  error: { active: "无法", done: "无法", failed: "无法" },
};

export function formatToolSummary(tool: ToolRun): string {
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

export function getToolOutputLineCount(tool: ToolRun): number {
  const visible = tool.output ? tool.output.split("\n").length : 0;
  return visible + (tool.truncatedLines ?? 0);
}

export function createResultDescriptor(tool: ToolRun): ResultDescriptor | null {
  if (tool.status !== "completed" || !tool.output) return null;
  const target = extractToolTarget(tool.input);
  if (tool.name === "edit" || tool.name === "write") {
    const summary = summarizeDiff(tool.output);
    if (summary) {
      return {
        type: "diff",
        title:
          summary.files > 1
            ? `${summary.files} 个文件发生修改`
            : (target ?? "文件发生修改"),
        description: `新增 ${summary.added} 行，删除 ${summary.removed} 行`,
        actionLabel: "查看 Diff",
        content: tool.output,
      };
    }
    return {
      type: "file",
      title: target ?? "文件结果",
      description: tool.name === "edit" ? "文件修改已完成" : "文件创建已完成",
      actionLabel: "预览",
      content: tool.output,
    };
  }
  if (tool.name === "bash") {
    const lines = getToolOutputLineCount(tool);
    return {
      type: "log",
      title: "命令执行结果",
      description: `${lines} 行输出`,
      actionLabel: "查看日志",
      content: tool.output,
    };
  }
  if ((tool.name === "web_search" || tool.name === "fetch_content") && target) {
    return {
      type: "link",
      title: target,
      description: "外部资料已获取",
      actionLabel: "查看内容",
      content: tool.output,
    };
  }
  return null;
}

function summarizeDiff(output: string) {
  if (!output.includes("@@") && !/^\+\+\+ /m.test(output) && !/^--- /m.test(output)) {
    return null;
  }
  const files = new Set<string>();
  let added = 0;
  let removed = 0;
  for (const line of output.split("\n")) {
    if (line.startsWith("+++ ")) {
      files.add(line.slice(4).replace(/^[ab]\//, "").split("\t")[0] ?? line);
    } else if (line.startsWith("+")) {
      added += 1;
    } else if (line.startsWith("--- ")) {
      continue;
    } else if (line.startsWith("-")) {
      removed += 1;
    }
  }
  return { added, files: files.size, removed };
}

export function serializeConversation(
  title: string,
  messages: ConversationMessage[],
): string {
  const sections = messages.flatMap((message) => {
    if (!message.content.trim()) return [];
    const role = message.role === "user" ? "用户" : "Pi";
    return [`## ${role}\n\n${message.content.trim()}`];
  });
  return [`# ${title}`, ...sections].join("\n\n");
}
