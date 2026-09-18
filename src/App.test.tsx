import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import App from "./App";

describe("App", () => {
  it("展示产品身份与工程状态", () => {
    render(<App />);

    expect(screen.getByRole("heading", { name: "Lure" })).toBeInTheDocument();
    expect(screen.getByText("Pi 桌面客户端")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("工程基础设施已就绪");
  });
});
