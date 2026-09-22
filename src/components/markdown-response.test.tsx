import { render, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { MarkdownResponse } from "./markdown-response";

describe("Markdown 回复", () => {
  it("只占满可用宽度，不强制撑满消息高度", () => {
    const { container } = render(<MarkdownResponse>正文</MarkdownResponse>);
    const markdown = container.firstElementChild;

    expect(markdown).toHaveClass("w-full", "min-w-0");
    expect(markdown).not.toHaveClass("h-full", "size-full");
  });

  it("代码内容默认不显示行号", async () => {
    const { container } = render(
      <MarkdownResponse mode="static">{"```text\nproject/\n└── src/\n```"}</MarkdownResponse>,
    );

    await waitFor(() =>
      expect(container.querySelector('[data-streamdown="code-block-body"]')).toBeInTheDocument(),
    );
    expect(container.querySelector('[class*="counter-increment"]')).not.toBeInTheDocument();
  });
});
