import { describe, expect, it } from "vitest";
import {
  DEFAULT_MAX_TOKENS,
  parseConfigObject,
  serializeConfig,
} from "./config.js";

function parse(json: string) {
  return parseConfigObject(JSON.parse(json));
}

describe("config 反序列化：camelCase / snake_case 别名与默认值", () => {
  it("accepts camelCase keys", () => {
    const c = parse(`{"agents":{"defaults":{"maxTokens":4096}}}`);
    expect(c.agents.defaults.maxTokens).toBe(4096);
  });

  it("accepts snake_case keys", () => {
    const c = parse(`{"agents":{"defaults":{"max_tokens":4096}}}`);
    expect(c.agents.defaults.maxTokens).toBe(4096);
  });

  it("partial config fills defaults", () => {
    const c = parse(`{"agents":{"defaults":{"model":"x/y"}}}`);
    expect(c.agents.defaults.model).toBe("x/y");
    expect(c.agents.defaults.provider).toBe("auto");
    expect(c.agents.defaults.maxTokens).toBe(DEFAULT_MAX_TOKENS);
  });

  it("unknown keys are ignored", () => {
    const c = parse(`{"agents":{"defaults":{"model":"x/y"}},"unknown":1}`);
    expect(c.agents.defaults.model).toBe("x/y");
  });

  it("modelPresets root key accepts camelCase and snake_case", () => {
    const camel = parse(
      `{"modelPresets":{"fast":{"model":"openai/gpt-4.1","provider":"openai"}}}`,
    );
    expect(camel.modelPresets["fast"]?.model).toBe("openai/gpt-4.1");
    expect(camel.modelPresets["fast"]?.maxTokens).toBe(DEFAULT_MAX_TOKENS);

    const snake = parse(`{"model_presets":{"fast":{"model":"x/y"}}}`);
    expect(snake.modelPresets["fast"]?.model).toBe("x/y");
  });

  it("modelPreset field accepts snake_case alias", () => {
    const c = parse(
      `{"agents":{"defaults":{"model_preset":"fast"}},"model_presets":{"fast":{"model":"x/y"}}}`,
    );
    expect(c.agents.defaults.modelPreset).toBe("fast");
  });

  it("providers accepts camelCase and defaults enabled", () => {
    const c = parse(
      `{"providers":{"deepseek":{"apiKey":"k","apiBase":"https://b/v1"}}}`,
    );
    expect(c.providers["deepseek"]?.apiKey).toBe("k");
    expect(c.providers["deepseek"]?.apiBase).toBe("https://b/v1");
    expect(c.providers["deepseek"]?.enabled).toBe(true);
  });
});

describe("config 序列化：camelCase 输出", () => {
  it("emits camelCase keys with 2-space indent", () => {
    const text = serializeConfig(parseConfigObject({}));
    expect(text).toContain('"maxTokens"');
    expect(text).not.toContain("max_tokens");
    expect(text).toContain('\n  "');
  });
});
