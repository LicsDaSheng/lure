import { describe, expect, it } from "vitest";
import { defaultConfig, parseConfigObject, type Config, type ProviderConfig } from "@lure/schema";
import {
  providerApiKey,
  resolvePreset,
  resolveProvider,
  validateConfig,
  type PresetError,
} from "./resolve.js";
import type { Result } from "neverthrow";

function parse(json: string): Config {
  return parseConfigObject(JSON.parse(json));
}

function unwrap<T>(r: Result<T, unknown>): T {
  if (r.isErr()) throw r.error;
  return r.value;
}

function configWithProvider(name: string, provider: ProviderConfig): Config {
  const config = defaultConfig();
  return { ...config, providers: { ...config.providers, [name]: provider } };
}

describe("resolve_preset / validate", () => {
  it("resolve_preset returns defaults when no preset", () => {
    const config = defaultConfig();
    const resolved = unwrap(resolvePreset(config));
    const defaults = config.agents.defaults;
    expect(resolved.model).toBe(defaults.model);
    expect(resolved.provider).toBe(defaults.provider);
    expect(resolved.maxTokens).toBe(defaults.maxTokens);
    expect(resolved.contextWindowTokens).toBe(defaults.contextWindowTokens);
    expect(resolved.temperature).toBe(defaults.temperature);
    expect(resolved.reasoningEffort).toBe(defaults.reasoningEffort);
  });

  it("legacy defaults without presets still resolves", () => {
    const config = parse(
      `{"agents":{"defaults":{"model":"openai/gpt-4.1","provider":"openai","maxTokens":4096,"contextWindowTokens":128000,"temperature":0.2,"reasoningEffort":"low"}}}`,
    );
    expect(config.agents.defaults.modelPreset).toBeUndefined();
    expect(Object.keys(config.modelPresets)).toHaveLength(0);

    const resolved = unwrap(resolvePreset(config));
    expect(resolved.model).toBe("openai/gpt-4.1");
    expect(resolved.provider).toBe("openai");
    expect(resolved.maxTokens).toBe(4096);
    expect(resolved.contextWindowTokens).toBe(128000);
    expect(resolved.temperature).toBe(0.2);
    expect(resolved.reasoningEffort).toBe("low");
  });

  it("resolve_preset returns active preset", () => {
    const config = parse(
      `{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai","maxTokens":4096,"contextWindowTokens":32768,"temperature":0.5,"reasoningEffort":"low"}},"agents":{"defaults":{"modelPreset":"fast"}}}`,
    );
    const resolved = unwrap(resolvePreset(config));
    expect(resolved.model).toBe("openai/gpt-4.1");
    expect(resolved.maxTokens).toBe(4096);
    expect(resolved.contextWindowTokens).toBe(32768);
    expect(resolved.temperature).toBe(0.5);
    expect(resolved.reasoningEffort).toBe("low");
  });

  it("default preset is agents defaults even when named preset active", () => {
    const config = parse(
      `{"agents":{"defaults":{"model":"openai/gpt-4.1","provider":"openai","modelPreset":"fast"}},"modelPresets":{"fast":{"model":"openai/gpt-4.1-mini","provider":"openai"}}}`,
    );
    expect(unwrap(resolvePreset(config)).model).toBe("openai/gpt-4.1-mini");
    expect(unwrap(resolvePreset(config, "default")).model).toBe("openai/gpt-4.1");
  });

  it("resolve_preset can target named preset without activating", () => {
    const config = parse(
      `{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai"},"deep":{"model":"anthropic/claude-opus-4-5","provider":"anthropic"}},"agents":{"defaults":{"modelPreset":"fast"}}}`,
    );
    const resolved = unwrap(resolvePreset(config, "deep"));
    expect(resolved.model).toBe("anthropic/claude-opus-4-5");
    expect(resolved.provider).toBe("anthropic");
  });

  it("resolve_preset rejects unknown named preset", () => {
    const r = resolvePreset(defaultConfig(), "missing");
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect((r.error as PresetError).presetName).toBe("missing");
  });

  it("validate rejects unknown active preset", () => {
    const r = validateConfig(parse(`{"agents":{"defaults":{"modelPreset":"unknown"}}}`));
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect(r.error).toContain("'unknown' not found");
  });

  it("validate rejects reserved default preset name", () => {
    const r = validateConfig(parse(`{"modelPresets":{"default":{"model":"custom-model"}}}`));
    expect(r.isErr()).toBe(true);
    if (r.isErr()) expect(r.error).toContain("reserved");
  });

  it("validate accepts explicit default preset name", () => {
    const config = parse(
      `{"agents":{"defaults":{"model":"openai/gpt-4.1","modelPreset":"default"}}}`,
    );
    expect(validateConfig(config).isOk()).toBe(true);
    expect(unwrap(resolvePreset(config)).model).toBe("openai/gpt-4.1");
  });
});

describe("resolve_provider", () => {
  it("applies apiBase override", () => {
    const config = configWithProvider("deepseek", {
      apiBase: "https://proxy.example/v1",
      enabled: true,
    });
    const resolved = resolveProvider(config, "deepseek-chat", "auto");
    expect(resolved?.name).toBe("deepseek");
    expect(resolved?.apiBase).toBe("https://proxy.example/v1");
  });

  it("falls back to registry default apiBase", () => {
    const resolved = resolveProvider(defaultConfig(), "deepseek-chat", "auto");
    expect(resolved?.name).toBe("deepseek");
    expect(resolved?.apiBase).toBe("https://api.deepseek.com");
  });

  it("skips disabled in auto", () => {
    const config = configWithProvider("openai", { enabled: false });
    expect(resolveProvider(config, "gpt-4o", "auto")).toBeUndefined();
  });

  it("honors forced even if disabled", () => {
    const config = configWithProvider("anthropic", { enabled: false });
    const resolved = resolveProvider(config, "some-unmatched-model", "anthropic");
    expect(resolved?.name).toBe("anthropic");
  });

  it("unknown forced is undefined", () => {
    expect(resolveProvider(defaultConfig(), "gpt-4o", "nonexistent")).toBeUndefined();
  });

  it("provider apiKey reads config entry", () => {
    const config = configWithProvider("deepseek", {
      apiKey: "sk-from-config",
      enabled: true,
    });
    expect(providerApiKey(config, "deepseek")).toBe("sk-from-config");
    expect(providerApiKey(config, "openai")).toBeUndefined();
  });
});
