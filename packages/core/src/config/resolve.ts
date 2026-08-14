//! config 派生逻辑：validate / resolve_preset / resolve_provider（对齐 schema.rs 方法）。

import { err, ok, type Result } from "neverthrow";
import type { Config, ModelPresetConfig } from "@lure/schema";
import { DEFAULT_PRESET_NAME } from "@lure/schema";
import { findByName, PROVIDERS, type ProviderSpec } from "./registry.js";

export interface ResolvedProvider {
  name: string;
  apiBase: string;
}

export class PresetError extends Error {
  constructor(readonly presetName: string) {
    super(`model_preset '${presetName}' not found in model_presets`);
    this.name = "PresetError";
  }
}

/// 校验 preset 约束（对齐 `_validate_model_preset`）。
export function validateConfig(config: Config): Result<undefined, string> {
  if (Object.prototype.hasOwnProperty.call(config.modelPresets, DEFAULT_PRESET_NAME)) {
    return err(`model_preset name '${DEFAULT_PRESET_NAME}' is reserved for agents.defaults`);
  }
  const active = config.agents.defaults.modelPreset;
  if (
    active !== undefined &&
    active !== "" &&
    active !== DEFAULT_PRESET_NAME &&
    !Object.prototype.hasOwnProperty.call(config.modelPresets, active)
  ) {
    return err(`model_preset '${active}' not found in model_presets`);
  }
  return ok(undefined);
}

/// 由 `agents.defaults` 字段构造隐式 `default` preset。
export function resolveDefaultPreset(config: Config): ModelPresetConfig {
  const d = config.agents.defaults;
  return {
    model: d.model,
    provider: d.provider,
    maxTokens: d.maxTokens,
    contextWindowTokens: d.contextWindowTokens,
    temperature: d.temperature,
    reasoningEffort: d.reasoningEffort,
  };
}

/// 解析生效的 model 参数：命名 preset 或隐式默认。
export function resolvePreset(
  config: Config,
  name?: string,
): Result<ModelPresetConfig, PresetError> {
  const resolvedName = name !== undefined ? name : config.agents.defaults.modelPreset;
  if (resolvedName === undefined || resolvedName === "" || resolvedName === DEFAULT_PRESET_NAME) {
    return ok(resolveDefaultPreset(config));
  }
  const preset = config.modelPresets[resolvedName];
  if (preset === undefined) {
    return err(new PresetError(resolvedName));
  }
  return ok(preset);
}

/// config 驱动地解析模型应使用的 provider。
export function resolveProvider(
  config: Config,
  model: string,
  forced: string,
): ResolvedProvider | undefined {
  let spec: ProviderSpec | undefined;
  if (forced !== "auto") {
    spec = findByName(forced);
  } else {
    const slash = model.indexOf("/");
    if (slash >= 0) {
      spec = findByName(model.slice(0, slash));
    }
    if (spec === undefined) {
      const lower = model.toLowerCase();
      spec = PROVIDERS.find(
        (s) => providerEnabled(config, s.name) && s.keywords.some((kw) => lower.includes(kw)),
      );
    }
  }
  if (spec === undefined) return undefined;
  return { name: spec.name, apiBase: providerApiBase(config, spec.name, spec.defaultApiBase) };
}

/// provider 的显式 api_key（仅来自 config；env 回落由调用方负责）。
export function providerApiKey(config: Config, name: string): string | undefined {
  return config.providers[name]?.apiKey;
}

function providerEnabled(config: Config, name: string): boolean {
  return config.providers[name]?.enabled ?? true;
}

function providerApiBase(config: Config, name: string, def: string): string {
  return config.providers[name]?.apiBase ?? def;
}
