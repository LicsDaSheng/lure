import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import App from "./App";

describe("App", () => {
  it("使用 AI Elements 展示对话流和输入框", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Lure" })).toBeInTheDocument();
    expect(screen.getByRole("log")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("给 Pi 发送消息…")).toBeInTheDocument();
  });

  it("提交输入后将用户消息加入对话流", async () => {
    render(<App />);
    const input = screen.getByPlaceholderText("给 Pi 发送消息…");

    fireEvent.change(input, { target: { value: "检查当前项目" } });
    fireEvent.submit(input.closest("form")!);

    expect(await screen.findByText("检查当前项目")).toBeInTheDocument();
  });
});
