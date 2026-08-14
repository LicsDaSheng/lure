//! OpenAI-compatible provider（对齐 crates/lure-core/src/provider/openai.rs）。

import type {
  CompletionRequest,
  GenerationSettings,
  Json,
  LlmResponse,
  StreamChunk,
  ToolCall,
} from "./types.js";
import { ProviderError } from "./types.js";
import type { HttpRequest, HttpResponse, HttpTransport } from "./http.js";
import { asArray, asObject, asString } from "./json.js";
import { parseSseLine, StreamAssembler } from "./stream.js";
import { normalizeUsage } from "./usage.js";

export class OpenAiCompatProvider {
  constructor(
    private readonly baseUrl: string,
    private readonly apiKey: string | undefined,
    private readonly model: string,
    private readonly transport: HttpTransport,
    private providerName?: string,
  ) {}

  withProviderName(name: string): this {
    this.providerName = name;
    return this;
  }

  defaultModel(): string {
    return this.model;
  }

  private httpRequest(request: CompletionRequest, stream: boolean): HttpRequest {
    const body = buildChatRequest(request.model, request.messages, request.settings);
    applyProviderReasoning(body, this.providerName, request.settings);
    if (request.tools.length > 0) {
      body["tools"] = request.tools;
    }
    if (stream) {
      body["stream"] = true;
      body["stream_options"] = { include_usage: true };
    }
    const url = `${this.baseUrl.replace(/\/+$/, "")}/chat/completions`;
    const headers: [string, string][] = [["content-type", "application/json"]];
    if (this.apiKey !== undefined) {
      headers.push(["authorization", `Bearer ${this.apiKey}`]);
    }
    return { url, headers, body };
  }

  async complete(request: CompletionRequest): Promise<LlmResponse> {
    const httpRequest = this.httpRequest(request, false);
    let response: HttpResponse;
    try {
      response = await this.transport.postJson(httpRequest);
    } catch (e) {
      throw new ProviderError("transport", String(e));
    }
    const llm = parseChatResponse(response);
    logLlmResponse(llm);
    return llm;
  }

  async completeStreaming(
    request: CompletionRequest,
    onDelta: (chunk: StreamChunk) => void,
  ): Promise<LlmResponse> {
    const httpRequest = this.httpRequest(request, true);

    const assembler = new StreamAssembler();
    let raw = "";
    let status: number;
    try {
      status = await this.transport.postJsonStreaming(httpRequest, (line) => {
        raw += line + "\n";
        try {
          const chunk = parseSseLine(line);
          if (chunk !== null) {
            assembler.push(chunk);
            onDelta(chunk);
          }
        } catch {
          // 单行解析失败按容错忽略（SSE 常含 keep-alive/注释）。
        }
      });
    } catch (e) {
      throw new ProviderError("transport", String(e));
    }

    const error = statusError(status, raw);
    if (error) throw error;

    const llm = assembler.finish();
    logLlmResponse(llm);
    return llm;
  }
}

/// 构建 OpenAI-compatible chat completions 请求体。
export function buildChatRequest(
  model: string,
  messages: Json[],
  settings: GenerationSettings,
): Record<string, Json> {
  return {
    model,
    messages,
    temperature: settings.temperature,
    max_tokens: Math.max(1, settings.maxTokens),
  };
}

/// 把通用 reasoning 设置映射为 provider 原生的请求形状（DeepSeek thinking 开关）。
function applyProviderReasoning(
  body: Record<string, Json>,
  providerName: string | undefined,
  settings: GenerationSettings,
): void {
  if (providerName === undefined || providerName.toLowerCase() !== "deepseek") return;
  if (settings.reasoningEffort?.toLowerCase() === "none") {
    body["thinking"] = { type: "disabled" };
  }
}

/// 按状态码分类并解析 chat completions 响应。
export function parseChatResponse(response: HttpResponse): LlmResponse {
  const error = statusError(response.status, response.body);
  if (error) throw error;

  let value: Json;
  try {
    value = JSON.parse(response.body);
  } catch (e) {
    throw new ProviderError("response", `响应不是合法 JSON: ${e}`);
  }

  const root = asObject(value) ?? {};
  const choice = asArray(root["choices"])?.[0];
  if (choice === undefined) {
    throw new ProviderError("response", "响应缺少 choices");
  }

  const message = asObject(asObject(choice)?.["message"]);
  const content = asString(message?.["content"]);
  const reasoningContent = asString(message?.["reasoning_content"]);
  const toolCalls = parseToolCalls(message);
  const finishReason = asString(asObject(choice)?.["finish_reason"]) ?? "stop";
  const usage = normalizeUsage(asObject(root["usage"]) ?? {});

  return { content, reasoningContent, finishReason, usage, toolCalls };
}

/// 非 2xx 状态映射为结构化错误；2xx 返回 `undefined`。
function statusError(status: number, body: string): ProviderError | undefined {
  if (status >= 200 && status <= 299) return undefined;
  if (status === 401 || status === 403) return new ProviderError("auth", snippet(body), status);
  if (status === 429) return new ProviderError("rate_limited", snippet(body), status);
  if (status >= 500 && status <= 599) return new ProviderError("server", snippet(body), status);
  return new ProviderError("api", snippet(body), status);
}

/// 从 `message.tool_calls` 解析工具调用；缺失或非数组时返回空。
function parseToolCalls(message: Record<string, Json> | undefined): ToolCall[] {
  const calls = asArray(message?.["tool_calls"]);
  if (!calls) return [];
  return calls.flatMap((call) => {
    const callObj = asObject(call);
    const fn = asObject(callObj?.["function"]);
    const name = asString(fn?.["name"]);
    if (name === undefined) return [];
    const id = asString(callObj?.["id"]) ?? "";
    const args = asString(fn?.["arguments"]) ?? "";
    return [{ id, name, arguments: args }];
  });
}

/// 截断过长的错误响应体，避免污染错误消息。
function snippet(body: string): string {
  const MAX = 500;
  if (body.length <= MAX) return body;
  return `${body.slice(0, MAX)}…`;
}

/// 调试打印 provider 返回的完整结果（think/content/tool_call）。
function logLlmResponse(response: LlmResponse): void {
  if (response.reasoningContent) {
    console.error(`[llm] think:\n${response.reasoningContent}`);
  }
  if (response.content) {
    console.error(`[llm] content:\n${response.content}`);
  }
  response.toolCalls.forEach((tool, i) => {
    console.error(`[llm] tool_call[${i}] id=${tool.id} name=${tool.name}\n  args: ${tool.arguments}`);
  });
  console.error(`[llm] finish_reason=${response.finishReason}`);
}
