import { describe, expect, it } from "vitest";
import { separateGluedReasoningBlocks } from "./reasoning-blocks";

describe("separateGluedReasoningBlocks", () => {
  it("把粘连的 **** 标题运行拆成两个标题", () => {
    expect(separateGluedReasoningBlocks("**One****Two**")).toBe(
      "**One**\n\n**Two**",
    );
  });

  it("把 prose 与标题的粘连拆开", () => {
    expect(separateGluedReasoningBlocks("interaction!**Two**")).toBe(
      "interaction!\n\n**Two**",
    );
  });

  it("幂等：已有换行不重复处理", () => {
    const once = separateGluedReasoningBlocks("**One**\n\n**Two**");
    expect(separateGluedReasoningBlocks(once)).toBe(once);
  });

  it("正常加粗不受影响", () => {
    expect(separateGluedReasoningBlocks("**bold** 普通文本")).toBe(
      "**bold** 普通文本",
    );
  });
});
