import { describe, expect, it } from "vitest";

import type { ConversationMessage, ToolRun } from "@/features/pi-connection/reducer";
import {
  createResultDescriptor,
  formatToolSummary,
  getToolOutputLineCount,
  serializeConversation,
} from "./presentation";

const completedTool: ToolRun = {
  id: "tool-1",
  name: "edit",
  status: "completed",
  input: JSON.stringify({ path: "src/App.tsx" }),
  output: "updated",
  truncatedLines: null,
};

describe("主工作区展示适配", () => {
  it("将已知工具转换为用户可理解的摘要", () => {
    expect(formatToolSummary({ ...completedTool, name: "read", status: "running" })).toBe(
      "正在读取 src/App.tsx",
    );
    expect(formatToolSummary(completedTool)).toBe("已修改 src/App.tsx");
  });

  it("只为可确定类型的工具结果创建结果卡片", () => {
    expect(createResultDescriptor(completedTool)).toMatchObject({
      type: "file",
      title: "src/App.tsx",
      actionLabel: "预览",
    });
    expect(
      createResultDescriptor({ ...completedTool, name: "unknown" }),
    ).toBeNull();
  });

  it("合并已显示和截断的输出行数", () => {
    expect(
      getToolOutputLineCount({
        ...completedTool,
        output: "one\ntwo",
        truncatedLines: 8,
      }),
    ).toBe(10);
  });

  it("把 Diff 输出识别为变更卡片并统计增删行数", () => {
    const descriptor = createResultDescriptor({
      ...completedTool,
      output: [
        "--- a/src/App.tsx",
        "+++ b/src/App.tsx",
        "@@ -1,2 +1,3 @@",
        "+added one",
        "-removed one",
        " context",
      ].join("\n"),
    });

    expect(descriptor).toMatchObject({
      type: "diff",
      title: "src/App.tsx",
      description: "新增 1 行，删除 1 行",
      actionLabel: "查看 Diff",
    });
  });

  it("将可见会话导出为 Markdown", () => {
    const messages: ConversationMessage[] = [
      {
        id: "user-1",
        role: "user",
        kind: "message",
        content: "检查项目",
        thinking: "",
        blocks: [],
        tools: [],
      },
      {
        id: "assistant-1",
        role: "assistant",
        kind: "message",
        content: "检查完成",
        thinking: "内部过程",
        blocks: [],
        tools: [completedTool],
      },
    ];

    const markdown = serializeConversation("项目检查", messages);
    expect(markdown).toContain("# 项目检查");
    expect(markdown).toContain("## 用户\n\n检查项目");
    expect(markdown).toContain("## Pi\n\n检查完成");
    expect(markdown).not.toContain("内部过程");
  });
});
