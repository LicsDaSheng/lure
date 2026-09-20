import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Button } from "./button";
import { Input } from "./input";
import { Textarea } from "./textarea";

describe("全局微交互", () => {
  it("按钮使用包提供的 hover、focus 和 active 效果", () => {
    render(<Button>执行</Button>);

    const button = screen.getByRole("button", { name: "执行" });
    expect(button).toHaveClass("cs-hover-brighten");
    expect(button).toHaveClass("cs-focus-pop");
    expect(button).toHaveClass("cs-active-scale-98");
    expect(button).toHaveClass("cs-tap-highlight-none");
  });

  it("输入控件使用平滑且可感知的 focus 效果", () => {
    render(
      <>
        <Input aria-label="标题" />
        <Textarea aria-label="内容" />
      </>,
    );

    for (const control of [
      screen.getByRole("textbox", { name: "标题" }),
      screen.getByRole("textbox", { name: "内容" }),
    ]) {
      expect(control).toHaveClass("cs-smooth-interaction-fast");
      expect(control).toHaveClass("cs-focus-pop");
    }
  });
});
