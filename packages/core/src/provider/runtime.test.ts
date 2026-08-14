import { describe, expect, it } from "vitest";
import type { Result } from "neverthrow";
import { defaultConfig, type Config, type ModelPresetConfig } from "@lure/schema";
import { ModelRuntimeResolver } from "./runtime.js";

function unwrap<T>(r: Result<T, unknown>): T {
  if (r.isErr()) throw r.error;
  return r.value;
}

function preset(model: string, provider: string): ModelPresetConfig {
  return { model, provider, maxTokens: 1024, contextWindowTokens: 64_000, temperature: 0.5 };
}

function configWithPreset(name: string, p: ModelPresetConfig): Config {
  return { ...defaultConfig(), modelPresets: { [name]: p } };
}

describe("ModelRuntimeResolver", () => {
  it("admit default resolves runtime from agent defaults", () => {
    const resolver = new ModelRuntimeResolver(defaultConfig());
    expect(resolver.active()).toBeUndefined();

    const runtime = unwrap(resolver.admit());
    expect(runtime.presetName).toBe("default");
    expect(runtime.provider.providerName).toBe("anthropic");
    expect(runtime.provider.model).toBe("anthropic/claude-opus-4-5");
    expect(runtime.provider.apiBase).toBe("https://api.anthropic.com/v1");
    expect(runtime.settings).toEqual({ temperature: 0.1, maxTokens: 8192, reasoningEffort: undefined });
    expect(runtime.generation).toBe(1);

    expect(resolver.active()?.presetName).toBe("default");
    expect(resolver.active()?.generation).toBe(1);
  });

  it("admit is idempotent and returns cached generation", () => {
    const resolver = new ModelRuntimeResolver(defaultConfig());
    expect(unwrap(resolver.admit()).generation).toBe(1);
    expect(unwrap(resolver.admit()).generation).toBe(1);
  });

  it("admit named preset tracks provider and settings", () => {
    const fast = preset("deepseek-chat", "auto");
    fast.maxTokens = 1024;
    fast.temperature = 0.5;
    fast.reasoningEffort = "low";

    const resolver = new ModelRuntimeResolver(configWithPreset("fast", fast));
    const runtime = unwrap(resolver.admit("fast"));

    expect(runtime.presetName).toBe("fast");
    expect(runtime.provider.providerName).toBe("deepseek");
    expect(runtime.provider.apiBase).toBe("https://api.deepseek.com");
    expect(runtime.settings).toEqual({ temperature: 0.5, maxTokens: 1024, reasoningEffort: "low" });
  });

  it("refresh rebuilds and bumps generation", () => {
    const resolver = new ModelRuntimeResolver(defaultConfig());
    expect(unwrap(resolver.admit()).generation).toBe(1);

    const refreshed = unwrap(resolver.refresh());
    expect(refreshed.presetName).toBe("default");
    expect(refreshed.generation).toBe(2);
    expect(resolver.active()?.generation).toBe(2);
  });

  it("invalidate clears active and forces rebuild", () => {
    const resolver = new ModelRuntimeResolver(defaultConfig());
    expect(unwrap(resolver.admit()).generation).toBe(1);

    resolver.invalidate("default");
    expect(resolver.active()).toBeUndefined();

    expect(unwrap(resolver.admit()).generation).toBe(2);
  });

  it("admit unknown preset errors and leaves active untouched", () => {
    const resolver = new ModelRuntimeResolver(defaultConfig());
    const r = resolver.admit("missing");
    expect(r.isErr()).toBe(true);
    if (r.isErr()) {
      expect(r.error.kind).toBe("preset");
      expect(r.error.presetName).toBe("missing");
    }
    expect(resolver.active()).toBeUndefined();
  });

  it("admit errors when provider cannot be matched", () => {
    const resolver = new ModelRuntimeResolver(configWithPreset("weird", preset("mystery-model", "auto")));
    const r = resolver.admit("weird");
    expect(r.isErr()).toBe(true);
    if (r.isErr()) {
      expect(r.error.kind).toBe("provider_not_found");
      expect(r.error.model).toBe("mystery-model");
      expect(r.error.provider).toBe("auto");
    }
  });

  it("admit honors forced provider name", () => {
    const resolver = new ModelRuntimeResolver(configWithPreset("forced", preset("some-custom-model", "groq")));
    const runtime = unwrap(resolver.admit("forced"));
    expect(runtime.provider.providerName).toBe("groq");
    expect(runtime.provider.apiBase).toBe("https://api.groq.com/openai/v1");
  });

  it("switching presets tracks active and caches each", () => {
    const resolver = new ModelRuntimeResolver(configWithPreset("smart", preset("gpt-4o", "auto")));

    const defaultRt = unwrap(resolver.admit());
    expect(resolver.active()?.presetName).toBe("default");

    const smartRt = unwrap(resolver.admit("smart"));
    expect(smartRt.provider.providerName).toBe("openai");
    expect(resolver.active()?.presetName).toBe("smart");

    const defaultAgain = unwrap(resolver.admit());
    expect(defaultAgain.generation).toBe(defaultRt.generation);
    expect(resolver.active()?.presetName).toBe("default");
  });

  it("admit applies provider apiBase override from config", () => {
    const config = configWithPreset("fast", preset("deepseek-chat", "auto"));
    config.providers["deepseek"] = { apiBase: "https://proxy.example/v1", enabled: true };

    const resolver = new ModelRuntimeResolver(config);
    const runtime = unwrap(resolver.admit("fast"));
    expect(runtime.provider.providerName).toBe("deepseek");
    expect(runtime.provider.apiBase).toBe("https://proxy.example/v1");
  });

  it("admit errors when matched provider disabled", () => {
    const config = configWithPreset("smart", preset("gpt-4o", "auto"));
    config.providers["openai"] = { enabled: false };

    const resolver = new ModelRuntimeResolver(config);
    const r = resolver.admit("smart");
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect(r.error.kind).toBe("provider_not_found");
  });

  it("admit none follows agent default preset pointer", () => {
    const config = configWithPreset("fast", preset("deepseek-chat", "auto"));
    config.agents.defaults.modelPreset = "fast";

    const resolver = new ModelRuntimeResolver(config);
    const runtime = unwrap(resolver.admit());
    expect(runtime.presetName).toBe("fast");
    expect(runtime.provider.providerName).toBe("deepseek");
  });
});
