import { beforeEach, describe, expect, it } from "vitest";

import {
  readDraft,
  readLastDirectory,
  readTitle,
  writeDraft,
  writeLastDirectory,
  writeTitle,
} from "./local-preferences";

beforeEach(() => {
  localStorage.clear();
});

describe("本地界面偏好", () => {
  it("按工作目录分别保存和恢复未发送草稿", () => {
    writeDraft("/tmp/alpha", "alpha 草稿");
    writeDraft("/tmp/beta", "beta 草稿");

    expect(readDraft("/tmp/alpha")).toBe("alpha 草稿");
    expect(readDraft("/tmp/beta")).toBe("beta 草稿");
    expect(readDraft(null)).toBe("");
  });

  it("清空草稿时移除对应记录", () => {
    writeDraft("/tmp/alpha", "草稿");
    writeDraft("/tmp/alpha", "");

    expect(readDraft("/tmp/alpha")).toBe("");
    expect(localStorage.getItem("lure:draft:/tmp/alpha")).toBeNull();
  });

  it("保存任务标题与上次工作目录", () => {
    writeTitle("/tmp/alpha", "重构布局");
    writeLastDirectory("/tmp/alpha");

    expect(readTitle("/tmp/alpha")).toBe("重构布局");
    expect(readTitle("/tmp/beta")).toBeNull();
    expect(readLastDirectory()).toBe("/tmp/alpha");
  });
});