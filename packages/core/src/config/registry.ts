//! 最小 provider registry 与选择顺序（对齐 crates/lure-core/src/provider/registry.rs）。

export interface ProviderSpec {
  name: string;
  keywords: string[];
  defaultApiBase: string;
}

/// 代表性 provider 子集，顺序对齐上游 registry 的相对次序。
export const PROVIDERS: ProviderSpec[] = [
  { name: "openrouter", keywords: ["openrouter"], defaultApiBase: "https://openrouter.ai/api/v1" },
  { name: "anthropic", keywords: ["anthropic", "claude"], defaultApiBase: "https://api.anthropic.com/v1" },
  { name: "openai", keywords: ["openai", "gpt"], defaultApiBase: "https://api.openai.com/v1" },
  { name: "deepseek", keywords: ["deepseek"], defaultApiBase: "https://api.deepseek.com" },
  { name: "gemini", keywords: ["gemini", "gemma"], defaultApiBase: "https://generativelanguage.googleapis.com/v1beta/openai/" },
  { name: "moonshot", keywords: ["moonshot", "kimi"], defaultApiBase: "https://api.moonshot.ai/v1" },
  { name: "mistral", keywords: ["mistral", "magistral", "ministral", "codestral", "devstral"], defaultApiBase: "https://api.mistral.ai/v1" },
  { name: "groq", keywords: ["groq"], defaultApiBase: "https://api.groq.com/openai/v1" },
  { name: "novita", keywords: ["novita"], defaultApiBase: "https://api.novita.ai/openai" },
];

function normalize(value: string): string {
  return value.toLowerCase().replace(/-/g, "_");
}

/// 按名精确查找 provider（大小写、连字符/下划线归一）。
export function findByName(name: string): ProviderSpec | undefined {
  const normalized = normalize(name);
  return PROVIDERS.find((spec) => normalize(spec.name) === normalized);
}

/// 解析模型应使用的 provider（不含 config 驱动的启用/ api_base，见 resolveProvider）。
export function matchProvider(model: string, forced: string): ProviderSpec | undefined {
  if (forced !== "auto") return findByName(forced);

  const slash = model.indexOf("/");
  if (slash >= 0) {
    const prefix = findByName(model.slice(0, slash));
    if (prefix) return prefix;
  }

  const lower = model.toLowerCase();
  return PROVIDERS.find((spec) => spec.keywords.some((kw) => lower.includes(kw)));
}
