import { beforeEach, describe, expect, it } from "vitest";

import {
  readLastDirectory,
  readProjects,
  readTitle,
  writeLastDirectory,
  writeProjects,
  writeTitle,
} from "./sessions-preferences";
import { readDraft, writeDraft } from "@/features/conversation/conversation-drafts";

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

  it("保存项目列表并忽略损坏的本地记录", () => {
    const projects = [{ directory: "/tmp/lure-project", name: "lure-project" }];
    writeProjects(projects);
    expect(readProjects()).toEqual(projects);

    localStorage.setItem("lure:projects", "not-json");
    expect(readProjects()).toEqual([]);
  });
});
