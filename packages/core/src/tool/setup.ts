//! config 驱动的工具注册（对齐 crates/lure-core/src/tool/setup.rs）。

import { err, ok, type Result } from "neverthrow";
import type { Config } from "@lure/schema";
import { EditFileTool, ReadFileTool, WriteFileTool } from "./file.js";
import { ToolRegistry } from "./registry.js";
import { ExecPolicy, ExecTool } from "./shell.js";

export class ToolSetupError extends Error {
  constructor(message: string) {
    super(`exec 策略正则无效: ${message}`);
    this.name = "ToolSetupError";
  }
}

/// 由 config + workspace 构建工具注册表。
export function registryFromConfig(config: Config, workspace: string): Result<ToolRegistry, ToolSetupError> {
  const registry = new ToolRegistry();
  registry.register(new ReadFileTool(workspace));
  registry.register(new WriteFileTool(workspace));
  registry.register(new EditFileTool(workspace));

  const exec = config.tools.exec;
  if (exec.enabled) {
    try {
      const policy = ExecPolicy.new(exec.allow, exec.deny);
      registry.register(new ExecTool(policy, workspace));
    } catch (e) {
      return err(new ToolSetupError(String(e)));
    }
  }

  return ok(registry);
}
