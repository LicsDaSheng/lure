//! `ensureWorkspace`：初始化 Lure 数据目录与 workspace 模板（对齐原 CLI 的 run_onboard）。

import fs from "node:fs";
import path from "node:path";
import { defaultConfig } from "@lure/schema";
import { defaultLureRoot } from "../config/paths.js";
import { saveConfig } from "../config/loader.js";
import { DEFAULT_AGENTS, DEFAULT_GITIGNORE, DEFAULT_HEARTBEAT, DEFAULT_SOUL, DEFAULT_USER } from "./templates.js";

export interface OnboardSummary {
  root: string;
  configPath: string;
  workspace: string;
}

/// 幂等初始化 Lure 数据目录与 workspace 模板；不覆盖已有用户文件。
export function ensureWorkspace(root?: string): OnboardSummary {
  const actualRoot = root ?? defaultLureRoot();
  const workspace = path.join(actualRoot, "workspace");

  for (const dir of [
    path.join(actualRoot, "cli-apps"),
    path.join(actualRoot, "cron"),
    path.join(actualRoot, "history"),
    path.join(actualRoot, "webui"),
    workspace,
    path.join(workspace, "cron"),
    path.join(workspace, "memory"),
    path.join(workspace, "prompts"),
    path.join(workspace, "sessions"),
    path.join(workspace, "skills"),
    path.join(workspace, "triggers"),
  ]) {
    fs.mkdirSync(dir, { recursive: true });
  }

  const configPath = path.join(actualRoot, "config.json");
  if (!fs.existsSync(configPath)) {
    const config = defaultConfig();
    config.agents.defaults.workspace =
      actualRoot === defaultLureRoot() ? "~/.lure/workspace" : workspace;
    const result = saveConfig(config, configPath);
    if (result.isErr()) throw result.error;
  }

  writeIfMissing(path.join(workspace, ".gitignore"), DEFAULT_GITIGNORE);
  writeIfMissing(path.join(workspace, "SOUL.md"), DEFAULT_SOUL);
  writeIfMissing(path.join(workspace, "USER.md"), DEFAULT_USER);
  writeIfMissing(path.join(workspace, "AGENTS.md"), DEFAULT_AGENTS);
  writeIfMissing(path.join(workspace, "HEARTBEAT.md"), DEFAULT_HEARTBEAT);
  writeIfMissing(path.join(workspace, "memory", "MEMORY.md"), "# Memory\n\n");

  return { root: actualRoot, configPath, workspace };
}

function writeIfMissing(p: string, contents: string): void {
  if (fs.existsSync(p)) return;
  fs.mkdirSync(path.dirname(p), { recursive: true });
  fs.writeFileSync(p, contents);
}
