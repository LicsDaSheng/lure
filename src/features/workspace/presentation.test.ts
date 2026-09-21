import { describe, expect, it } from "vitest";

import type { ConversationMessage, ToolPart } from "@/features/pi-connection/reducer";
import {
  formatToolSummary,
  getToolOutputLineCount,
  serializeConversation,
  type ToolView,
} from "./presentation";

const completedView: ToolView = {
  id: "tool-1",
  name: "edit",
  status: "completed",
  input: JSON.stringify({ path: "src/App.tsx" }),
  output: "updated",
  truncatedLines: null,
};

const completedToolPart: ToolPart = {
  id: "tool-1",
  type: "tool",
  contentIndex: 0,
  toolCallId: "tool-1",
  name: "edit",
  status: "completed",
  input: JSON.stringify({ path: "src/App.tsx" }),
  output: "updated",
  truncatedLines: null,
};

describe("主工作区展示适配", () => {
  it("将已知工具转换为用户可理解的摘要", () => {
    expect(formatToolSummary({ ...completedView, name: "read", status: "running" })).toBe(
      "正在读取 src/App.tsx",
    );
    expect(formatToolSummary(completedView)).toBe("已修改 src/App.tsx");
  });

  it("合并已显示和截断的输出行数", () => {
    expect(
      getToolOutputLineCount({
        ...completedView,
        output: "one\ntwo",
        truncatedLines: 8,
      }),
    ).toBe(10);
  });

  it("将可见会话导出为 Markdown", () => {
    const messages: ConversationMessage[] = [
      {
        id: "user-1",
        role: "user",
        parts: [{ id: "text-0", type: "text", contentIndex: 0, text: "检查项目" }],
      },
      {
        id: "assistant-1",
        role: "assistant",
        parts: [
          { id: "thinking-0", type: "thinking", contentIndex: 0, text: "内部过程" },
          { id: "text-1", type: "text", contentIndex: 1, text: "检查完成" },
          completedToolPart,
        ],
      },
    ];

    const markdown = serializeConversation("项目检查", messages);
    expect(markdown).toContain("# 项目检查");
    expect(markdown).toContain("## 用户\n\n检查项目");
    expect(markdown).toContain("## Pi\n\n检查完成");
    expect(markdown).not.toContain("内部过程");
  });
});
