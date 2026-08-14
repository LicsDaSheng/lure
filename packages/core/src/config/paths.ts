//! 配置与 workspace 路径解析。默认数据目录为 `~/.lure`；只做纯路径解析，不创建目录。

import { homedir } from "node:os";
import path from "node:path";

/// 返回用户主目录；无法解析属于启动期不可恢复错误。
export function homeDir(): string {
  const home = homedir();
  if (!home) {
    throw new Error("无法解析用户主目录（HOME 未设置）");
  }
  return home;
}

/// Lure 自己的数据目录根：`~/.lure`。
export function defaultLureRoot(): string {
  return path.join(homeDir(), ".lure");
}

/// 默认 config 文件路径：`~/.lure/config.json`。
export function defaultConfigPath(): string {
  return path.join(defaultLureRoot(), "config.json");
}

/// 默认 workspace 路径：`~/.lure/workspace`。
export function defaultWorkspace(): string {
  return path.join(defaultLureRoot(), "workspace");
}

/// 展开路径中的 `~` 前缀为用户主目录。
export function expandUser(p: string): string {
  if (p === "~") return homeDir();
  if (p.startsWith("~/")) return path.join(homeDir(), p.slice(2));
  return p;
}

/// 解析 workspace 路径：`undefined` 回落默认，否则 `~` 展开。
export function resolveWorkspace(workspace?: string): string {
  return workspace !== undefined ? expandUser(workspace) : defaultWorkspace();
}

/// 判断给定 workspace 是否解析到默认 workspace。
export function isDefaultWorkspace(workspace?: string): boolean {
  return resolveWorkspace(workspace) === defaultWorkspace();
}
