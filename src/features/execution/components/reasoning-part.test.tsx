import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ReasoningPart } from "./reasoning-part";

describe("思考过程展示", () => {
  it("流式期间显示思考中，完成后显示已耗时", () => {
    const { rerender } = render(
      <ReasoningPart streaming text="先确认配置来源" />,
    );

    expect(screen.getByRole("button", { name: /思考中/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );

    rerender(<ReasoningPart streaming={false} text="先确认配置来源" />);

    expect(
      screen.getByRole("button", { name: /已思考 1 秒/ }),
    ).toBeInTheDocument();
  });

  it("历史消息没有耗时信息时显示思考过程", () => {
    render(<ReasoningPart streaming={false} text="先确认配置来源" />);

    expect(
      screen.getByRole("button", { name: /思考过程/ }),
    ).toBeInTheDocument();
  });

  it("展开后渲染思考内容", () => {
    render(<ReasoningPart streaming={false} text="先确认配置来源" />);

    fireEvent.click(screen.getByRole("button", { name: /思考过程/ }));

    expect(screen.getByText("先确认配置来源")).toBeInTheDocument();
  });
});
