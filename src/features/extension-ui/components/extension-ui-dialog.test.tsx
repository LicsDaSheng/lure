import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import type { ExtensionUiRequest } from "@/lib/pi-rpc/types";

import { ExtensionUiDialog } from "./extension-ui-dialog";

function makeRequest(
  overrides: Partial<ExtensionUiRequest> = {},
): ExtensionUiRequest {
  return {
    requestId: "ui-1",
    method: "select",
    title: "选择范围",
    message: null,
    options: [],
    placeholder: null,
    defaultValue: null,
    ...overrides,
  };
}

function renderDialog(request: ExtensionUiRequest, onRespond = vi.fn()) {
  render(<ExtensionUiDialog onRespond={onRespond} request={request} />);
  return { onRespond };
}

describe("ExtensionUiDialog", () => {
  it("单选请求以单选框平铺选项，默认选中第一项", () => {
    renderDialog(makeRequest({ options: ["1. 当前文件", "2. 整个项目"] }));

    const group = screen.getByRole("radiogroup", { name: "选择一项" });
    const radios = screen.getAllByRole("radio");
    expect(group).toBeInTheDocument();
    expect(radios).toHaveLength(2);
    expect(screen.getByRole("radio", { name: "1. 当前文件" })).toBeChecked();
    expect(
      screen.getByRole("radio", { name: "2. 整个项目" }),
    ).not.toBeChecked();
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
  });

  it("点选 Type something. 后立即进入扩展后续输入请求", async () => {
    const onRespond = vi.fn().mockResolvedValue(undefined);
    const { rerender } = render(
      <ExtensionUiDialog
        onRespond={onRespond}
        request={makeRequest({
          options: ["1. 选项 A", "2. Type something."],
        })}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: "2. Type something." }));
    await waitFor(() =>
      expect(onRespond).toHaveBeenCalledWith("2. Type something.", false),
    );

    rerender(
      <ExtensionUiDialog
        onRespond={onRespond}
        request={makeRequest({
          requestId: "ui-2",
          method: "input",
          title: "请输入自定义回答",
          options: [],
        })}
      />,
    );
    const input = screen.getByRole("textbox", { name: "输入内容" });
    fireEvent.change(input, { target: { value: "我的回答" } });
    fireEvent.click(screen.getByRole("button", { name: "提交输入" }));
    await waitFor(() =>
      expect(onRespond).toHaveBeenLastCalledWith("我的回答", false),
    );
  });

  it("中文自定义回答选项也会立即进入输入流程", async () => {
    const { onRespond } = renderDialog(
      makeRequest({ options: ["1. 选项 A", "2. 输入内容"] }),
    );

    fireEvent.click(screen.getByRole("radio", { name: "2. 输入内容" }));
    await waitFor(() =>
      expect(onRespond).toHaveBeenCalledWith("2. 输入内容", false),
    );
  });

  it("普通选择的末项不会自动提交", () => {
    const onRespond = vi.fn();
    renderDialog(makeRequest({ options: ["红色", "蓝色"] }), onRespond);
    fireEvent.click(screen.getByRole("radio", { name: "蓝色" }));
    expect(onRespond).not.toHaveBeenCalled();
  });

  it("单选提交回传选中的选项文本", async () => {
    const { onRespond } = renderDialog(
      makeRequest({ options: ["1. 当前文件", "2. 整个项目"] }),
    );

    fireEvent.click(screen.getByRole("radio", { name: "2. 整个项目" }));
    fireEvent.click(screen.getByRole("button", { name: "确认选择" }));

    await waitFor(() =>
      expect(onRespond).toHaveBeenCalledWith("2. 整个项目", false),
    );
  });

  it("文本输入请求完整展示多行提示文本，并透传输入内容与占位提示", async () => {
    const title =
      "多选问题：挑选选项\n\n1. 选项 A\n2. 选项 B\n\n请输入编号，逗号分隔";
    const { onRespond } = renderDialog(
      makeRequest({
        method: "input",
        title,
        placeholder: "例如 1,3",
      }),
    );

    // 多行 title 在正文完整可见，而不是被压缩进标题栏
    expect(screen.getByText(/多选问题：挑选选项/)).toBeInTheDocument();
    expect(screen.getByText(/1\. 选项 A/)).toBeInTheDocument();
    expect(screen.getByText(/请输入编号，逗号分隔/)).toBeInTheDocument();

    const input = screen.getByRole("textbox", { name: "输入内容" });
    expect(input).toHaveAttribute("placeholder", "例如 1,3");

    fireEvent.change(input, { target: { value: "1,2" } });
    fireEvent.click(screen.getByRole("button", { name: "提交输入" }));

    await waitFor(() => expect(onRespond).toHaveBeenCalledWith("1,2", false));
  });

  it("文本输入允许空提交（多选空选择等语义由扩展定义）", async () => {
    const { onRespond } = renderDialog(
      makeRequest({ method: "input", title: "可留空的问题" }),
    );

    fireEvent.click(screen.getByRole("button", { name: "提交输入" }));

    await waitFor(() => expect(onRespond).toHaveBeenCalledWith("", false));
  });

  it("editor 请求使用多行编辑框", () => {
    renderDialog(makeRequest({ method: "editor", title: "编辑发布说明" }));

    const editor = screen.getByRole("textbox", { name: "编辑内容" });
    expect(editor.tagName).toBe("TEXTAREA");
  });

  it("确认请求回传确认结果", async () => {
    const { onRespond } = renderDialog(
      makeRequest({
        method: "confirm",
        title: "删除任务？",
        message: "该操作不可恢复。",
      }),
    );

    expect(screen.getByText("删除任务？")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "确认操作" }));

    await waitFor(() => expect(onRespond).toHaveBeenCalledWith(true, false));
  });

  it("取消回传 cancelled", async () => {
    const { onRespond } = renderDialog(
      makeRequest({ options: ["1. 当前文件"] }),
    );

    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    await waitFor(() => expect(onRespond).toHaveBeenCalledWith(null, true));
  });
});
