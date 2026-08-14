//! Provider 契约类型（对齐 crates/lure-core/src/provider/types.rs）。

/// JSON 值（对应 serde_json::Value，导航时经 json.ts 辅助函数窄化）。
export type Json = unknown;

export interface GenerationSettings {
  temperature: number;
  maxTokens: number;
  reasoningEffort?: string;
}

export function defaultGenerationSettings(): GenerationSettings {
  return { temperature: 0.7, maxTokens: 4096 };
}

/// 一次补全请求。
export interface CompletionRequest {
  model: string;
  messages: Json[];
  settings: GenerationSettings;
  tools: Json[];
}

/// provider 请求的一次工具调用（OpenAI function-calling 形状）。
export interface ToolCall {
  id: string;
  name: string;
  arguments: string;
}

/// 流式 tool_call 增量。
export interface ToolCallDelta {
  index: number;
  id?: string;
  name?: string;
  arguments?: string;
}

/// 一条流式增量。
export interface StreamChunk {
  contentDelta?: string;
  reasoningDelta?: string;
  toolCallDeltas: ToolCallDelta[];
  finishReason?: string;
  usage: Record<string, Json>;
}

/// provider 返回的补全响应。
export interface LlmResponse {
  content?: string;
  reasoningContent?: string;
  finishReason: string;
  usage: Record<string, Json>;
  toolCalls: ToolCall[];
}

export type ProviderErrorKind =
  | "request"
  | "transport"
  | "auth"
  | "rate_limited"
  | "server"
  | "api"
  | "response";

/// provider 结构化错误。
export class ProviderError extends Error {
  constructor(
    readonly kind: ProviderErrorKind,
    message: string,
    readonly status?: number,
  ) {
    super(providerErrorMessage(kind, message, status));
    this.name = "ProviderError";
  }
}

function providerErrorMessage(kind: ProviderErrorKind, message: string, status?: number): string {
  switch (kind) {
    case "request":
      return `provider 请求失败: ${message}`;
    case "transport":
      return `provider 传输失败: ${message}`;
    case "auth":
      return `provider 认证失败(${status}): ${message}`;
    case "rate_limited":
      return `provider 限流(${status}): ${message}`;
    case "server":
      return `provider 服务端错误(${status}): ${message}`;
    case "api":
      return `provider API 错误(${status}): ${message}`;
    case "response":
      return `provider 响应失败: ${message}`;
  }
}
