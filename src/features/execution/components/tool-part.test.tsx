import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ToolView } from "@/features/execution";
import { ToolCard } from "./tool-part";

function view(overrides: Partial<ToolView> = {}): ToolView {
  return {
    id: "tool-1",
    name: "read",
    status: "completed",
    input: JSON.stringify({ path: "README.md" }),
    output: "文件内容",
    truncatedLines: null,
    ...overrides,
  };
}

describe("工具执行展示", () => {
  it("折叠展示工具摘要与状态，展开后显示参数与输出", () => {
    render(<ToolCard view={view()} />);

    const trigger = screen.getByRole("button", { name: /已读取 README.md/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.queryByText("工具参数")).not.toBeInTheDocument();

    fireEvent.click(trigger);

    expect(
      screen.getByRole("button", { name: /已读取 README.md/ }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("输入")).toBeInTheDocument();
    expect(screen.getByText("输出")).toBeInTheDocument();
    expect(screen.getByText("文件内容")).toBeInTheDocument();
  });

  it("工具摘要使用紧凑行，并把操作名与工具名对齐展示", () => {
    const { container } = render(
      <ToolCard
        view={view({
          name: "str_replace_editor",
          input: JSON.stringify({ command: "str_replace", path: "test.md" }),
        })}
      />,
    );

    expect(screen.getByText("工具调用")).toBeInTheDocument();
    expect(
      screen.getByText("str_replace_editor · str_replace"),
    ).toBeInTheDocument();
    expect(container.firstElementChild).not.toHaveClass("rounded-xl", "border");
  });

  it("运行中的工具显示执行中状态", () => {
    render(<ToolCard view={view({ status: "running", output: "" })} />);

    expect(
      screen.getByRole("button", { name: /正在读取 README.md/ }),
    ).toBeInTheDocument();
    expect(screen.getByText("执行中")).toBeInTheDocument();
  });

  it("失败的工具把输出标记为错误信息", () => {
    render(<ToolCard view={view({ status: "error", output: "读取失败" })} />);

    fireEvent.click(screen.getByRole("button", { name: /无法读取 README.md/ }));

    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText("错误")).toBeInTheDocument();
    expect(screen.getByText("读取失败")).toBeInTheDocument();
  });

  it("长输出提供查看完整内容的入口", () => {
    const output = Array.from(
      { length: 30 },
      (_, index) => `第 ${index + 1} 行`,
    ).join("\n");
    render(<ToolCard view={view({ output })} />);

    fireEvent.click(screen.getByRole("button", { name: /已读取 README.md/ }));

    expect(
      screen.getByRole("button", { name: /查看完整 30 行输出/ }),
    ).toBeInTheDocument();
    expect(screen.getByText(/另有 20 行未显示/)).toBeInTheDocument();
  });

  it("短输出不提供预览入口", () => {
    render(<ToolCard view={view({ output: "两行\n内容" })} />);

    fireEvent.click(screen.getByRole("button", { name: /已读取 README.md/ }));

    expect(
      screen.queryByRole("button", { name: /查看完整/ }),
    ).not.toBeInTheDocument();
  });
});
