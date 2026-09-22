import { describe, expect, it } from "vitest";

import type { ConversationMessage, ToolPart } from "@/features/pi-connection";
import {
  executionProcessLabel,
  formatDuration,
  formatToolSummary,
  getToolOutputLineCount,
  relativeTimeLabel,
  serializeConversation,
  sessionTitle,
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

describe("执行过程折叠文案", () => {
  it("用可读的中文时长描述整次运行", () => {
    expect(formatDuration(400)).toBe("不到 1 秒");
    expect(formatDuration(50_000)).toBe("50 秒");
    expect(formatDuration(60_000)).toBe("1 分");
    expect(formatDuration(80_000)).toBe("1 分 20 秒");
  });

  it("同时说明状态与点击后的行为", () => {
    expect(
      executionProcessLabel({ durationMs: null, expanded: true, phase: "running", toolCount: 2 }),
    ).toBe("执行中");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "running", toolCount: 2 }),
    ).toBe("执行中 · 展开执行过程");
    expect(
      executionProcessLabel({ durationMs: 50_000, expanded: false, phase: "settled", toolCount: 1 }),
    ).toBe("用时 50 秒 · 展开执行过程");
    expect(
      executionProcessLabel({ durationMs: 50_000, expanded: true, phase: "settled", toolCount: 1 }),
    ).toBe("用时 50 秒 · 收起执行过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "settled", toolCount: 3 }),
    ).toBe("3 个工具调用 · 展开执行过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "settled", toolCount: 0 }),
    ).toBe("执行过程 · 展开执行过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "aborted", toolCount: 1 }),
    ).toBe("已停止 · 查看已完成过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: true, phase: "aborted", toolCount: 1 }),
    ).toBe("已停止 · 收起执行过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "aborted", toolCount: 0 }),
    ).toBe("已停止 · 查看已完成过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "truncated", toolCount: 1 }),
    ).toBe("响应被截断 · 查看过程");
    expect(
      executionProcessLabel({ durationMs: null, expanded: false, phase: "error", toolCount: 1 }),
    ).toBe("执行未完成 · 查看过程");
  });
});

describe("历史会话展示", () => {
  const session = {
    path: "/home/me/.pi/agent/sessions/--tmp--/2026-09-20T10-00-00-000Z_one.jsonl",
    id: "one",
    cwd: "/tmp/lure",
    name: null,
    parentSessionPath: null,
    createdAtMs: 1_789_898_400_000,
    modifiedAtMs: 1_789_898_400_000,
    messageCount: 2,
    firstMessage: null,
  };

  it("按会话名、首条用户消息、目录名的顺序生成列表标题", () => {
    expect(sessionTitle({ ...session, name: "重构任务" }, "lure")).toBe("重构任务");
    expect(sessionTitle({ ...session, firstMessage: "帮我看看\n第二行" }, "lure")).toBe("帮我看看");
    expect(sessionTitle(session, "lure")).toBe("lure");
    expect(
      sessionTitle({ ...session, firstMessage: `很长的首条消息${"啊".repeat(80)}` }, "lure").length,
    ).toBeLessThanOrEqual(61);
  });

  it("用可读的中文相对时间描述最近活动", () => {
    const now = 1_789_898_400_000;

    expect(relativeTimeLabel(now - 5_000, now)).toBe("刚刚");
    expect(relativeTimeLabel(now - 5 * 60_000, now)).toBe("5 分钟前");
    expect(relativeTimeLabel(now - 3 * 3_600_000, now)).toBe("3 小时前");
    expect(relativeTimeLabel(now - 2 * 86_400_000, now)).toBe("2 天前");
    expect(relativeTimeLabel(now - 30 * 86_400_000, now)).toBe("2026-08-21");
    expect(relativeTimeLabel(now + 60_000, now)).toBe("刚刚");
  });
});
