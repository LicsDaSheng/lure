import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import type { ToolCallMessagePartProps } from "@assistant-ui/react";

import type { ToolView } from "./presentation";
import { ToolCard, ToolPartCard, readToolView } from "./tool-part";

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
  it("直接消费工具调用部件携带的展示数据，不回查消息来源", () => {
    const output = Array.from({ length: 30 }, (_, index) => `第 ${index + 1} 行`).join("\n");
    render(
      <ToolPartCard
        {...({ artifact: view({ output }) } as ToolCallMessagePartProps)}
      />,
    );

    expect(screen.getByText("已完成")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /已读取 README.md/ }));
    expect(screen.getByText(/第 1 行/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /查看完整 30 行输出/ })).toBeInTheDocument();
  });

  it("缺少展示数据时不渲染工具卡", () => {
    const { container } = render(
      <ToolPartCard {...({} as ToolCallMessagePartProps)} />,
    );

    expect(container).toBeEmptyDOMElement();
  });

  it("展示数据形状不符时忽略该工具，不中断整条回复", () => {
    expect(readToolView(undefined)).toBeNull();
    expect(readToolView("tool-1")).toBeNull();
    expect(readToolView({ status: "completed" })).toBeNull();
    expect(readToolView({ id: "tool-1", name: "read", status: "unknown" })).toBeNull();
    expect(
      readToolView({ id: "tool-1", name: "read", status: "running" }),
    ).toEqual({
      id: "tool-1",
      name: "read",
      status: "running",
      input: "",
      output: "",
      truncatedLines: null,
    });
  });

  it("折叠展示工具摘要与状态，展开后显示参数与输出", () => {
    render(<ToolCard view={view()} />);

    const trigger = screen.getByRole("button", { name: /已读取 README.md/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.queryByText("工具参数")).not.toBeInTheDocument();

    fireEvent.click(trigger);

    expect(screen.getByRole("button", { name: /已读取 README.md/ })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
    expect(screen.getByText("工具参数")).toBeInTheDocument();
    expect(screen.getByText("工具输出")).toBeInTheDocument();
    expect(screen.getByText("文件内容")).toBeInTheDocument();
  });

  it("运行中的工具显示执行中状态", () => {
    render(<ToolCard view={view({ status: "running", output: "" })} />);

    expect(screen.getByRole("button", { name: /正在读取 README.md/ })).toBeInTheDocument();
    expect(screen.getByText("执行中")).toBeInTheDocument();
  });

  it("失败的工具把输出标记为错误信息", () => {
    render(<ToolCard view={view({ status: "error", output: "读取失败" })} />);

    fireEvent.click(screen.getByRole("button", { name: /无法读取 README.md/ }));

    expect(screen.getByText("失败")).toBeInTheDocument();
    expect(screen.getByText("错误信息")).toBeInTheDocument();
    expect(screen.getByText("读取失败")).toBeInTheDocument();
  });

  it("长输出提供查看完整内容的入口", () => {
    const output = Array.from({ length: 30 }, (_, index) => `第 ${index + 1} 行`).join("\n");
    render(<ToolCard view={view({ output })} />);

    fireEvent.click(screen.getByRole("button", { name: /已读取 README.md/ }));

    expect(screen.getByRole("button", { name: /查看完整 30 行输出/ })).toBeInTheDocument();
    expect(screen.getByText(/另有 20 行未显示/)).toBeInTheDocument();
  });

  it("短输出不提供预览入口", () => {
    render(<ToolCard view={view({ output: "两行\n内容" })} />);

    fireEvent.click(screen.getByRole("button", { name: /已读取 README.md/ }));

    expect(screen.queryByRole("button", { name: /查看完整/ })).not.toBeInTheDocument();
  });
});