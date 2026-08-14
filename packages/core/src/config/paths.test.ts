import { describe, expect, it } from "vitest";
import { homedir } from "node:os";
import path from "node:path";
import {
  defaultConfigPath,
  defaultWorkspace,
  isDefaultWorkspace,
  resolveWorkspace,
} from "./paths.js";

describe("config paths", () => {
  it("config path defaults to nanobot home", () => {
    expect(defaultConfigPath()).toBe(
      path.join(homedir(), ".nanobot", "config.json"),
    );
  });

  it("workspace defaults to nanobot home", () => {
    expect(resolveWorkspace()).toBe(
      path.join(homedir(), ".nanobot", "workspace"),
    );
  });

  it("custom workspace expands tilde", () => {
    expect(resolveWorkspace("~/custom-workspace")).toBe(
      path.join(homedir(), "custom-workspace"),
    );
  });

  it("isDefaultWorkspace distinguishes default and custom", () => {
    expect(isDefaultWorkspace()).toBe(true);
    expect(isDefaultWorkspace(defaultWorkspace())).toBe(true);
    expect(isDefaultWorkspace("~/custom-workspace")).toBe(false);
  });
});
