//! 配置读写：load / save 与结构化错误（对齐 crates/lure-core/src/config/loader.rs）。

import fs from "node:fs";
import path from "node:path";
import { err, ok, type Result } from "neverthrow";
import { parseConfigObject, serializeConfig, type Config } from "@lure/schema";
import { validateConfig } from "./resolve.js";

export type ConfigErrorKind = "read" | "parse" | "serialize" | "write" | "validation";

/// 配置读写的结构化错误。
export class ConfigError extends Error {
  constructor(
    readonly kind: ConfigErrorKind,
    readonly path?: string,
    readonly detail?: string,
  ) {
    super(configErrorMessage(kind, path, detail));
    this.name = "ConfigError";
  }
}

function configErrorMessage(
  kind: ConfigErrorKind,
  p: string | undefined,
  detail: string | undefined,
): string {
  switch (kind) {
    case "read":
      return `读取配置失败 ${p}: ${detail}`;
    case "parse":
      return `加载配置失败 ${p}: ${detail}`;
    case "serialize":
      return `序列化配置失败: ${detail}`;
    case "write":
      return `写入配置失败 ${p}: ${detail}`;
    case "validation":
      return `配置校验失败 ${p}: ${detail}`;
  }
}

/// 从文件加载配置；文件不存在时返回默认配置。
export function loadConfig(p: string): Result<Config, ConfigError> {
  if (!fs.existsSync(p)) {
    return ok(parseConfigObject({}));
  }

  let text: string;
  try {
    text = fs.readFileSync(p, "utf8");
  } catch (e) {
    return err(new ConfigError("read", p, String(e)));
  }

  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch (e) {
    return err(new ConfigError("parse", p, String(e)));
  }

  let config: Config;
  try {
    config = parseConfigObject(raw);
  } catch (e) {
    return err(new ConfigError("parse", p, String(e)));
  }

  const validation = validateConfig(config);
  if (validation.isErr()) {
    return err(new ConfigError("validation", p, validation.error));
  }
  return ok(config);
}

/// 将配置以约定格式原子写入文件。
export function saveConfig(config: Config, p: string): Result<undefined, ConfigError> {
  let json: string;
  try {
    json = serializeConfig(config);
  } catch (e) {
    return err(new ConfigError("serialize", undefined, String(e)));
  }
  try {
    writeTextAtomic(p, json);
  } catch (e) {
    return err(new ConfigError("write", p, String(e)));
  }
  return ok(undefined);
}

/// temp + fsync + rename 原子写文本；目标已存在时保留其权限位。
function writeTextAtomic(p: string, contents: string): void {
  const dir = path.dirname(p);
  fs.mkdirSync(dir, { recursive: true });

  const fileName = path.basename(p) || "config.json";
  const tmp = path.join(dir, `.${fileName}.tmp.${process.pid}`);

  try {
    const fd = fs.openSync(tmp, "w");
    try {
      fs.writeFileSync(fd, contents, "utf8");
      fs.fsyncSync(fd);
    } finally {
      fs.closeSync(fd);
    }
    preserveMode(p, tmp);
    fs.renameSync(tmp, p);
  } catch (e) {
    try {
      fs.rmSync(tmp, { force: true });
    } catch {
      // 清理半成品临时文件失败不影响原始错误。
    }
    throw e;
  }
}

/// 把既有目标文件的权限位复制到临时文件（unix）。
function preserveMode(existing: string, tmp: string): void {
  try {
    const mode = fs.statSync(existing).mode & 0o777;
    fs.chmodSync(tmp, mode);
  } catch {
    // 目标不存在或读取失败：无权限位可保留。
  }
}
