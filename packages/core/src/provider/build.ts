//! provider 接线：config + flag → 不可变 runtime → 真实 provider（对齐原 lure-cli 的 build_provider）。

import { err, ok, type Result } from "neverthrow";
import type { Config } from "@lure/schema";
import { providerApiKey } from "../config/resolve.js";
import { OpenAiCompatProvider } from "./openai.js";
import { EchoProvider } from "./echo.js";
import { FetchTransport } from "./transport.js";
import { ModelRuntimeResolver, type LlmRuntime } from "./runtime.js";
import type { LlmProvider } from "./types.js";

/// 从 config + flag 解析出不可变 runtime；`--preset` 与 `--model` 互斥。
export function resolveRuntime(
  config: Config,
  preset?: string,
  model?: string,
): Result<LlmRuntime, string> {
  if (preset !== undefined && model !== undefined) {
    return err("--preset 与 --model 互斥，只能二选一");
  }
  let effective = config;
  let selected: string | undefined;
  if (preset !== undefined) {
    selected = preset;
  } else if (model !== undefined) {
    effective = {
      ...config,
      agents: {
        ...config.agents,
        defaults: { ...config.agents.defaults, model, provider: "auto" },
      },
    };
  }
  return new ModelRuntimeResolver(effective).admit(selected).mapErr((e) => e.message);
}

/// 由 runtime 的 provider 快照构造真实 provider（api_key 回落环境变量 `<PROVIDER>_API_KEY`）。
export function buildProviderFromRuntime(
  config: Config,
  runtime: LlmRuntime,
): Result<OpenAiCompatProvider, string> {
  const providerName = runtime.provider.providerName;
  const envKey = `${providerName.toUpperCase()}_API_KEY`;
  const apiKey = providerApiKey(config, providerName) ?? process.env[envKey];
  if (apiKey === undefined) {
    return err(`缺少 API key：config.providers.${providerName}.apiKey 或环境变量 ${envKey}`);
  }
  return ok(
    new OpenAiCompatProvider(
      runtime.provider.apiBase,
      apiKey,
      runtime.provider.model,
      new FetchTransport(),
    ).withProviderName(providerName),
  );
}

/// 构建 provider：`--model echo` 返回离线 Echo，否则由 config/preset/model 解析真实 provider。
export function buildProvider(
  config: Config,
  preset?: string,
  model?: string,
): Result<LlmProvider, string> {
  if (preset !== undefined && model !== undefined) {
    return err("--preset 与 --model 互斥，只能二选一");
  }
  if (model === "echo") return ok(new EchoProvider());

  const runtime = resolveRuntime(config, preset, model);
  if (runtime.isErr()) return err(runtime.error);
  return buildProviderFromRuntime(config, runtime.value);
}
