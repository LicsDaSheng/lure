//! 配置 schema：zod 复刻，字段名/别名/默认值与 serde 字节级一致。
//! 序列化输出 camelCase；反序列化同时接受 camelCase 与 snake_case；
//! 缺失字段回落默认值；未知字段忽略。
//!
//! 可变的默认值（空对象/空数组/嵌套默认对象）经 `.transform` 每次解析生成新鲜实例，
//! 避免多个解析结果共享同一引用（对应 Rust `Config::default()` 每次返回新对象）。

import { z } from "zod";

// ---- 默认值常量（对齐 crates/lure-core/src/config/schema.rs）----
export const DEFAULT_MODEL = "anthropic/claude-opus-4-5";
export const DEFAULT_WORKSPACE = "~/.nanobot/workspace";
export const DEFAULT_PROVIDER = "auto";
export const DEFAULT_MAX_TOKENS = 8192;
export const DEFAULT_CONTEXT_WINDOW_TOKENS = 200_000;
export const DEFAULT_TEMPERATURE = 0.1;
export const DEFAULT_PRESET_NAME = "default";

// ---- snake_case → camelCase 键变换（等价 serde alias_generator=to_camel + 显式 alias）----
function camelizeKeys(input: unknown): unknown {
  if (input !== null && typeof input === "object" && !Array.isArray(input)) {
    const out: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(input as Record<string, unknown>)) {
      out[key.replace(/_([a-z])/g, (_m, c: string) => c.toUpperCase())] = value;
    }
    return out;
  }
  return input;
}

// ---- 类型（与 serde 结构的 camelCase 输出对齐）----
export interface ExecToolConfig {
  enabled: boolean;
  allow: string[];
  deny: string[];
}

export interface ToolsConfig {
  exec: ExecToolConfig;
}

export interface AgentDefaults {
  workspace: string;
  model: string;
  provider: string;
  modelPreset?: string;
  maxTokens: number;
  contextWindowTokens: number;
  temperature: number;
  reasoningEffort?: string;
}

export interface AgentsConfig {
  defaults: AgentDefaults;
}

export interface ModelPresetConfig {
  label?: string;
  model: string;
  provider: string;
  maxTokens: number;
  contextWindowTokens: number;
  temperature: number;
  reasoningEffort?: string;
}

export interface ProviderConfig {
  apiKey?: string;
  apiBase?: string;
  enabled: boolean;
}

export interface Config {
  agents: AgentsConfig;
  modelPresets: Record<string, ModelPresetConfig>;
  providers: Record<string, ProviderConfig>;
  tools: ToolsConfig;
}

function defaultAgentDefaults(): AgentDefaults {
  return {
    workspace: DEFAULT_WORKSPACE,
    model: DEFAULT_MODEL,
    provider: DEFAULT_PROVIDER,
    maxTokens: DEFAULT_MAX_TOKENS,
    contextWindowTokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
    temperature: DEFAULT_TEMPERATURE,
  };
}

const ExecToolConfigSchema = z.object({
  enabled: z.boolean().default(false),
  allow: z.array(z.string()).optional().transform((v) => v ?? []),
  deny: z.array(z.string()).optional().transform((v) => v ?? []),
});

const ToolsConfigSchema = z.object({
  exec: ExecToolConfigSchema.optional().transform(
    (v) => v ?? { enabled: false, allow: [], deny: [] },
  ),
});

const AgentDefaultsSchema = z.preprocess(
  camelizeKeys,
  z.object({
    workspace: z.string().default(DEFAULT_WORKSPACE),
    model: z.string().default(DEFAULT_MODEL),
    provider: z.string().default(DEFAULT_PROVIDER),
    modelPreset: z.string().optional(),
    maxTokens: z.number().int().min(0).default(DEFAULT_MAX_TOKENS),
    contextWindowTokens: z
      .number()
      .int()
      .min(0)
      .default(DEFAULT_CONTEXT_WINDOW_TOKENS),
    temperature: z.number().default(DEFAULT_TEMPERATURE),
    reasoningEffort: z.string().optional(),
  }),
);

const AgentsConfigSchema = z.object({
  defaults: AgentDefaultsSchema.optional().transform((v) => v ?? defaultAgentDefaults()),
});

const ModelPresetConfigSchema = z.preprocess(
  camelizeKeys,
  z.object({
    label: z.string().optional(),
    model: z.string(),
    provider: z.string().default(DEFAULT_PROVIDER),
    maxTokens: z.number().int().min(0).default(DEFAULT_MAX_TOKENS),
    contextWindowTokens: z
      .number()
      .int()
      .min(0)
      .default(DEFAULT_CONTEXT_WINDOW_TOKENS),
    temperature: z.number().default(DEFAULT_TEMPERATURE),
    reasoningEffort: z.string().optional(),
  }),
);

const ProviderConfigSchema = z.preprocess(
  camelizeKeys,
  z.object({
    apiKey: z.string().optional(),
    apiBase: z.string().optional(),
    enabled: z.boolean().default(true),
  }),
);

export const ConfigSchema = z.preprocess(
  camelizeKeys,
  z.object({
    agents: AgentsConfigSchema.optional().transform(
      (v) => v ?? { defaults: defaultAgentDefaults() },
    ),
    modelPresets: z
      .record(z.string(), ModelPresetConfigSchema)
      .optional()
      .transform((v) => v ?? {}),
    providers: z
      .record(z.string(), ProviderConfigSchema)
      .optional()
      .transform((v) => v ?? {}),
    tools: ToolsConfigSchema.optional().transform(
      (v) => v ?? { exec: { enabled: false, allow: [], deny: [] } },
    ),
  }),
);

/// 解析任意 JSON 对象为 `Config`（应用别名 + 默认值 + 未知字段忽略）。
export function parseConfigObject(raw: unknown): Config {
  return ConfigSchema.parse(raw);
}

/// 返回全默认 `Config`（每次返回新鲜实例）。
export function defaultConfig(): Config {
  return parseConfigObject({});
}

/// 以约定格式序列化 `Config`：camelCase、2 空格缩进、保留非 ASCII、省略 undefined 可选字段。
export function serializeConfig(config: Config): string {
  return JSON.stringify(config, null, 2);
}
