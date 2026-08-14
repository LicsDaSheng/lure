//! OpenAI-compatible SSE 流式解析与增量组装（对齐 crates/lure-core/src/provider/stream.rs）。

import type { Json, LlmResponse, StreamChunk, ToolCallDelta } from "./types.js";
import { ProviderError } from "./types.js";
import { asArray, asNumber, asObject, asString } from "./json.js";
import { normalizeUsage } from "./usage.js";

/// 解析一条 SSE 行为 `StreamChunk`；非 `data:` 行 / `[DONE]` / 空返回 `null`。
/// `data:` 行内容不是合法 JSON 时抛 [`ProviderError`]（调用方按容错忽略）。
export function parseSseLine(line: string): StreamChunk | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("data:")) return null;
  const data = trimmed.slice("data:".length).trim();
  if (data === "" || data === "[DONE]") return null;

  let value: Json;
  try {
    value = JSON.parse(data);
  } catch (e) {
    throw new ProviderError("response", `SSE chunk 不是合法 JSON: ${e}`);
  }

  const root = asObject(value) ?? {};
  const usage = asObject(root["usage"]) ?? {};

  const choice = asArray(root["choices"])?.[0];
  if (choice === undefined) {
    return { usage, toolCallDeltas: [] };
  }

  const delta = asObject(asObject(choice)?.["delta"]);
  const contentDelta = asString(delta?.["content"]);
  const reasoningDelta = asString(delta?.["reasoning_content"]);
  const toolCallDeltas = parseToolCallDeltas(delta);
  const finishReason = asString(asObject(choice)?.["finish_reason"]);

  return { contentDelta, reasoningDelta, toolCallDeltas, finishReason, usage };
}

function parseToolCallDeltas(delta: Record<string, Json> | undefined): ToolCallDelta[] {
  const calls = asArray(delta?.["tool_calls"]);
  if (!calls) return [];
  return calls.map((call) => {
    const callObj = asObject(call);
    const fn = asObject(callObj?.["function"]);
    return {
      index: asNumber(callObj?.["index"]) ?? 0,
      id: asString(callObj?.["id"]),
      name: asString(fn?.["name"]),
      arguments: asString(fn?.["arguments"]),
    };
  });
}

interface PartialToolCall {
  id: string;
  name: string;
  arguments: string;
}

/// 把有序流式增量折叠为最终 [`LlmResponse`]。
export class StreamAssembler {
  private content = "";
  private reasoning = "";
  private finishReason?: string;
  private toolCalls = new Map<number, PartialToolCall>();
  private usage: Record<string, Json> = {};

  /// 累积一个增量。
  push(chunk: StreamChunk): void {
    if (chunk.contentDelta !== undefined) this.content += chunk.contentDelta;
    if (chunk.reasoningDelta !== undefined) this.reasoning += chunk.reasoningDelta;
    if (chunk.finishReason !== undefined) this.finishReason = chunk.finishReason;
    if (Object.keys(chunk.usage).length > 0) this.usage = chunk.usage;

    for (const delta of chunk.toolCallDeltas) {
      let entry = this.toolCalls.get(delta.index);
      if (entry === undefined) {
        entry = { id: "", name: "", arguments: "" };
        this.toolCalls.set(delta.index, entry);
      }
      if (delta.id !== undefined) entry.id = delta.id;
      if (delta.name !== undefined) entry.name = delta.name;
      if (delta.arguments !== undefined) entry.arguments += delta.arguments;
    }
  }

  /// 收尾组装为 [`LlmResponse`]（无 finish_reason 时默认 `stop`）。
  finish(): LlmResponse {
    const content = this.content.length === 0 ? undefined : this.content;
    const reasoningContent = this.reasoning.length === 0 ? undefined : this.reasoning;
    const toolCalls = [...this.toolCalls.entries()]
      .sort(([a], [b]) => a - b)
      .map(([, p]) => ({ id: p.id, name: p.name, arguments: p.arguments }));

    return {
      content,
      reasoningContent,
      finishReason: this.finishReason ?? "stop",
      usage: normalizeUsage(this.usage),
      toolCalls,
    };
  }
}
