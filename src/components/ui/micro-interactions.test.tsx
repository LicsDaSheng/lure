import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Button } from "./button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./collapsible";

describe("全局微交互", () => {
  it("按钮使用明确的状态色以及 focus、active 微交互", () => {
    render(<Button>执行</Button>);

    const button = screen.getByRole("button", { name: "执行" });
    expect(button).toHaveClass("hover:bg-[var(--button-primary-hover)]");
    expect(button).toHaveClass("active:bg-[var(--button-primary-active)]");
    expect(button).toHaveClass("cs-focus-pop");
    expect(button).toHaveClass("cs-active-scale-98");
    expect(button).toHaveClass("cs-tap-highlight-none");
  });

  it("折叠触发器使用平滑且可感知的交互效果", () => {
    render(
      <Collapsible>
        <CollapsibleTrigger>展开</CollapsibleTrigger>
        <CollapsibleContent>内容</CollapsibleContent>
      </Collapsible>,
    );

    const trigger = screen.getByRole("button", { name: "展开" });
    expect(trigger).toHaveClass("cs-smooth-interaction-fast");
    expect(trigger).toHaveClass("cs-focus-pop");
  });
});
