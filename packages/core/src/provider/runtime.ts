//! Stateful model runtime resolver（对齐 crates/lure-core/src/provider/runtime.rs）。

import { err, ok, type Result } from "neverthrow";
import type { Config } from "@lure/schema";
import { DEFAULT_PRESET_NAME } from "@lure/schema";
import { PresetError, resolvePreset, resolveProvider } from "../config/resolve.js";
import type { GenerationSettings } from "./types.js";

/// 解析时刻捕获的 provider 身份快照（不可变）。
export interface ProviderSnapshot {
  providerName: string;
  apiBase: string;
  model: string;
}

/// 一次解析产出的不可变 LLM 运行时。
export interface LlmRuntime {
  presetName: string;
  provider: ProviderSnapshot;
  settings: GenerationSettings;
  generation: number;
}

/// resolver 结构化错误。
export class RuntimeError extends Error {
  constructor(
    readonly kind: "preset" | "provider_not_found",
    message: string,
    readonly presetName?: string,
    readonly model?: string,
    readonly provider?: string,
  ) {
    super(message);
    this.name = "RuntimeError";
  }
}

/// stateful model runtime resolver。
export class ModelRuntimeResolver {
  private readonly cache = new Map<string, LlmRuntime>();
  private activeName?: string;
  private nextGeneration = 1;

  constructor(private readonly config: Config) {}

  /// 当前 admitted 的 runtime（未 admit 过则为 `undefined`）。
  active(): LlmRuntime | undefined {
    return this.activeName !== undefined ? this.cache.get(this.activeName) : undefined;
  }

  /// 解析并选中一个 preset 为当前 runtime。
  admit(name?: string): Result<LlmRuntime, RuntimeError> {
    const canonical = this.canonicalName(name);
    if (canonical.isErr()) {
      return err(new RuntimeError("preset", canonical.error.message, canonical.error.presetName));
    }
    const key = canonical.value;
    if (!this.cache.has(key)) {
      const built = this.build(key);
      if (built.isErr()) return built;
      this.cache.set(key, built.value);
    }
    this.activeName = key;
    return ok(this.cache.get(key)!);
  }

  /// 强制重建某 preset 的 runtime 并选中它，`generation` 递增。
  refresh(name?: string): Result<LlmRuntime, RuntimeError> {
    const canonical = this.canonicalName(name);
    if (canonical.isErr()) {
      return err(new RuntimeError("preset", canonical.error.message, canonical.error.presetName));
    }
    const key = canonical.value;
    const built = this.build(key);
    if (built.isErr()) return built;
    this.cache.set(key, built.value);
    this.activeName = key;
    return ok(built.value);
  }

  /// 丢弃某 preset 的缓存；若正是当前 active，则清空 active。
  invalidate(presetName: string): void {
    this.cache.delete(presetName);
    if (this.activeName === presetName) this.activeName = undefined;
  }

  private canonicalName(name?: string): Result<string, PresetError> {
    const requested = name !== undefined ? name : this.config.agents.defaults.modelPreset;
    if (requested === undefined || requested === "" || requested === DEFAULT_PRESET_NAME) {
      return ok(DEFAULT_PRESET_NAME);
    }
    if (Object.prototype.hasOwnProperty.call(this.config.modelPresets, requested)) {
      return ok(requested);
    }
    return err(new PresetError(requested));
  }

  private build(canonical: string): Result<LlmRuntime, RuntimeError> {
    const requested = canonical === DEFAULT_PRESET_NAME ? undefined : canonical;
    const preset = resolvePreset(this.config, requested);
    if (preset.isErr()) {
      return err(new RuntimeError("preset", preset.error.message, preset.error.presetName));
    }
    const p = preset.value;

    const resolved = resolveProvider(this.config, p.model, p.provider);
    if (resolved === undefined) {
      return err(
        new RuntimeError(
          "provider_not_found",
          `无法为 model '${p.model}'(provider='${p.provider}') 匹配 provider`,
          undefined,
          p.model,
          p.provider,
        ),
      );
    }

    const generation = this.nextGeneration;
    this.nextGeneration += 1;
    return ok({
      presetName: canonical,
      provider: {
        providerName: resolved.name,
        apiBase: resolved.apiBase,
        model: p.model,
      },
      settings: {
        temperature: p.temperature,
        maxTokens: p.maxTokens,
        reasoningEffort: p.reasoningEffort,
      },
      generation,
    });
  }
}
