import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ConversationTurn, MessagePart, ToolPart } from "@/features/conversation";

import { AssistantTurn } from "./assistant-turn";

function textPart(text: string, contentIndex = 0): MessagePart {
  return { id: `text-${contentIndex}`, type: "text", contentIndex, text };
}

function toolPart(toolCallId: string, name = "read", label = "package.json"): ToolPart {
  return {
    id: toolCallId,
    type: "tool",
    contentIndex: 1,
    toolCallId,
    name,
    status: "completed",
    input: JSON.stringify({ path: label }),
    output: "{\"name\":\"lure\"}",
    truncatedLines: null,
  };
}

function turn(overrides: Partial<ConversationTurn> = {}): ConversationTurn {
  return {
    id: "user-1",
    user: {
      id: "user-1",
      role: "user",
      parts: [textPart("检查项目")],
    },
    process: [],
    result: null,
    isRunning: false,
    phase: "settled",
    durationMs: null,
    toolCount: 0,
    ...overrides,
  };
}

describe("响应组展示", () => {
  it("没有过程内容时不显示空折叠控件，直接展示结果", () => {
    render(
      <AssistantTurn
        turn={turn({
          result: {
            id: "assistant-1",
            kind: "final",
            parts: [
              {
                id: "text-0",
                type: "text",
                contentIndex: 0,
                text: "检查完成",
              },
            ],
            stopReason: "stop",
            errorMessage: null,
          },
        })}
      />,
    );

    expect(screen.queryByRole("button", { name: /执行过程|执行中|已停止/ })).not.toBeInTheDocument();
    expect(screen.getByText("检查完成")).toBeInTheDocument();
  });

  it("运行中过程默认展开，完成后自动折叠且最终结果保持可见", () => {
    const withProcess = turn({
      isRunning: true,
      phase: "running",
      process: [
        {
          id: "assistant-1",
          parts: [textPart("先读取配置"), toolPart("tool-1")],
          stopReason: null,
          errorMessage: null,
          isRunning: true,
        },
      ],
      toolCount: 1,
      result: null,
    });
    const { rerender } = render(<AssistantTurn turn={withProcess} />);

    const running = screen.getByRole("button", { name: "执行中" });
    expect(running).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("先读取配置")).toBeVisible();

    rerender(
      <AssistantTurn
        turn={{
          ...withProcess,
          isRunning: false,
          phase: "settled",
          durationMs: 50_000,
          result: {
            id: "assistant-2",
            kind: "final",
            parts: [{ id: "text-0", type: "text", contentIndex: 0, text: "配置已确认" }],
            stopReason: "stop",
            errorMessage: null,
          },
        }}
      />,
    );

    const settled = screen.getByRole("button", { name: "用时 50 秒 · 展开执行过程" });
    expect(settled).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("配置已确认")).toBeVisible();
    expect(screen.getByText("先读取配置")).not.toBeVisible();
  });

  it("折叠触发器是可聚焦的真实按钮，并与内容关联", () => {
    const withProcess = turn({
      phase: "settled",
      toolCount: 1,
      process: [
        {
          id: "assistant-1",
          parts: [toolPart("tool-1")],
          stopReason: null,
          errorMessage: null,
          isRunning: false,
        },
      ],
    });
    render(<AssistantTurn turn={withProcess} />);

    const toggle = screen.getByRole("button", { name: "1 个工具调用 · 展开执行过程" });
    expect(toggle.tagName).toBe("BUTTON");
    toggle.focus();
    expect(toggle).toHaveFocus();

    const contentId = toggle.getAttribute("aria-controls");
    expect(contentId).toBeTruthy();
    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAccessibleName("1 个工具调用 · 收起执行过程");
    expect(document.getElementById(contentId!)).not.toBeNull();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("展开总过程不影响各工具卡的独立展开状态", () => {
    const withProcess = turn({
      phase: "settled",
      toolCount: 1,
      process: [
        {
          id: "assistant-1",
          parts: [toolPart("tool-1")],
          stopReason: null,
          errorMessage: null,
          isRunning: false,
        },
      ],
    });
    render(<AssistantTurn turn={withProcess} />);

    fireEvent.click(screen.getByRole("button", { name: "1 个工具调用 · 展开执行过程" }));
    const tool = screen.getByRole("button", { name: /已读取 package.json/ });
    fireEvent.click(tool);
    expect(tool).toHaveAttribute("aria-expanded", "true");

    // 收起再展开总过程，工具卡保持自己已展开的选择。
    fireEvent.click(screen.getByRole("button", { name: "1 个工具调用 · 收起执行过程" }));
    fireEvent.click(screen.getByRole("button", { name: "1 个工具调用 · 展开执行过程" }));

    expect(screen.getByRole("button", { name: /已读取 package.json/ })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("外层过程展开时不会把已收起工具的箭头误显示为展开方向", () => {
    render(
      <AssistantTurn
        turn={turn({
          isRunning: true,
          phase: "running",
          process: [
            {
              id: "assistant-1",
              parts: [toolPart("tool-1")],
              stopReason: null,
              errorMessage: null,
              isRunning: true,
            },
          ],
          toolCount: 1,
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "执行中" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    const tool = screen.getByRole("button", { name: /已读取 package.json/ });
    expect(tool).toHaveAttribute("aria-expanded", "false");
    expect(tool).toHaveClass("group/trigger");
    expect(tool.querySelector(".lucide-chevron-right")).toHaveClass(
      "group-data-[state=open]/trigger:hidden",
    );
    expect(tool.querySelector(".lucide-chevron-down")).toHaveClass(
      "group-data-[state=open]/trigger:block",
    );
  });

  it("被停止或失败的响应说明发生了什么，不称为最终答案", () => {
    const { rerender } = render(
      <AssistantTurn
        turn={turn({
          phase: "aborted",
          result: {
            id: "assistant-1",
            kind: "partial",
            parts: [{ id: "text-0", type: "text", contentIndex: 0, text: "写了一半" }],
            stopReason: "aborted",
            errorMessage: null,
          },
        })}
      />,
    );
    expect(screen.getByText("任务已由你停止，已完成的内容仍然保留。")).toBeInTheDocument();
    expect(screen.getByText("写了一半")).toBeInTheDocument();

    rerender(
      <AssistantTurn
        turn={turn({
          phase: "error",
          result: {
            id: "assistant-2",
            kind: "partial",
            parts: [],
            stopReason: "error",
            errorMessage: "模型连接中断",
          },
        })}
      />,
    );
    expect(screen.getByText("这次执行未能完成：模型连接中断")).toBeInTheDocument();
  });

  it("过程中失败的中间轮次如实说明，不遮挡后续结果", () => {
    render(
      <AssistantTurn
        turn={turn({
          phase: "settled",
          process: [
            {
              id: "assistant-1",
              parts: [textPart("失败响应")],
              stopReason: "error",
              errorMessage: "超时",
              isRunning: false,
            },
          ],
          result: {
            id: "assistant-2",
            kind: "final",
            parts: [{ id: "text-0", type: "text", contentIndex: 0, text: "成功响应" }],
            stopReason: "stop",
            errorMessage: null,
          },
        })}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /展开执行过程/ }));

    expect(screen.getByText("这一次执行未能完成：超时")).toBeInTheDocument();
    expect(screen.getByText("成功响应")).toBeVisible();
  });
});
