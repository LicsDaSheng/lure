//! 完整 agent loop（对齐 crates/lure-core/src/agent/loop_run.rs）：多轮 tool-call 循环 +
//! session/memory 持久化。

import type { Json, LlmProvider, ToolCall } from "../provider/types.js";
import { defaultGenerationSettings, type GenerationSettings } from "../provider/types.js";
import type { LlmRuntime } from "../provider/runtime.js";
import { asString } from "../provider/json.js";
import { AgentRunner } from "./runner.js";
import type { ContextBuilder } from "./context.js";
import { SessionManager } from "../session/store.js";
import { MemoryStore, type ConsolidationOutcome, type DreamRunner } from "../memory/store.js";
import { ToolRegistry } from "../tool/registry.js";

export const MAX_TOOL_ITERATIONS = 8;
export const MAX_EMPTY_RETRIES = 2;
export const EMPTY_FINAL_RESPONSE_MESSAGE =
  "I completed the tool steps but couldn't produce a final answer. Please try again or narrow the task.";
export const FINALIZATION_RETRY_PROMPT =
  "Please provide your response to the user based on the conversation above.";

export interface InboundMessage {
  channel: string;
  chatId: string;
  content: string;
  metadata?: Record<string, Json>;
}

export type ProgressEvent =
  | { type: "turn_started"; sessionKey: string }
  | { type: "content_delta"; text: string }
  | { type: "reasoning_delta"; text: string }
  | { type: "tool_invoked"; name: string }
  | { type: "finalizing" }
  | { type: "final_response"; content: string };

export interface TurnOutcome {
  finalContent: string;
  reasoning?: string;
  progress: ProgressEvent[];
  stopReason: string;
  usage: Record<string, number>;
}

export class AgentError extends Error {
  constructor(readonly kind: "provider" | "session" | "memory", message: string) {
    super(agentErrorMessage(kind, message));
    this.name = "AgentError";
  }
}

function agentErrorMessage(kind: "provider" | "session" | "memory", message: string): string {
  if (kind === "provider") return `agent provider 错误: ${message}`;
  if (kind === "session") return `agent session 错误: ${message}`;
  return `agent memory 错误: ${message}`;
}

export class AgentLoop {
  private model?: string;
  private settings?: GenerationSettings;
  private tools?: ToolRegistry;
  private memory?: MemoryStore;

  constructor(
    private readonly provider: LlmProvider,
    private readonly sessions: SessionManager,
    private readonly context: ContextBuilder,
  ) {}

  withTools(tools: ToolRegistry): this {
    this.tools = tools;
    return this;
  }
  withMemory(memory: MemoryStore): this {
    this.memory = memory;
    return this;
  }
  withRuntime(runtime: LlmRuntime): this {
    this.model = runtime.provider.model;
    this.settings = runtime.settings;
    return this;
  }

  modelName(): string {
    return this.model ?? this.provider.defaultModel();
  }
  settingsValue(): GenerationSettings {
    return this.settings ?? defaultGenerationSettings();
  }
  sessionsManager(): SessionManager {
    return this.sessions;
  }

  async consolidate<R extends DreamRunner>(runner: R): Promise<ConsolidationOutcome | undefined> {
    return this.memory?.consolidate(runner);
  }

  async maybeConsolidate<R extends DreamRunner>(
    runner: R,
    minEntries: number,
  ): Promise<ConsolidationOutcome | undefined> {
    if (this.memory === undefined || !this.memory.shouldConsolidate(minEntries)) return undefined;
    return this.memory.consolidate(runner);
  }

  async run(
    input: InboundMessage,
    onProgress?: (e: ProgressEvent) => void,
  ): Promise<TurnOutcome> {
    const key = `${input.channel}:${input.chatId}`;
    const progress: ProgressEvent[] = [];
    const emit = (e: ProgressEvent) => {
      onProgress?.(e);
      progress.push(e);
    };
    emit({ type: "turn_started", sessionKey: key });

    this.sessions.getOrCreate(key).addMessage("user", input.content);

    const memoryContext = this.memory?.getMemoryContext();
    const context =
      memoryContext !== undefined && memoryContext !== ""
        ? this.context.withMemory(memoryContext)
        : this.context;

    if (this.memory !== undefined) this.memory.appendHistory(input.content, key);

    let finalContent = "";
    let finalReasoning: string | undefined;
    let emptyRetries = 0;
    const usage: Record<string, number> = {};

    for (let i = 0; i < MAX_TOOL_ITERATIONS; i++) {
      const history = this.sessions.getOrCreate(key).getHistory(0);
      const messages = context.build(history);
      const runner = new AgentRunner(this.provider, this.settingsValue()).withTools(
        this.tools?.getDefinitions() ?? [],
      );
      const response = await runner
        .runStreaming(this.modelName(), messages, (chunk) => {
          if (chunk.reasoningDelta) emit({ type: "reasoning_delta", text: chunk.reasoningDelta });
          if (chunk.contentDelta) emit({ type: "content_delta", text: chunk.contentDelta });
        })
        .catch((e) => {
          throw new AgentError("provider", String(e));
        });

      accumulateUsage(usage, response.usage);

      const content = response.content ?? "";
      const reasoning =
        response.reasoningContent !== undefined && response.reasoningContent !== ""
          ? response.reasoningContent
          : undefined;
      const runTools = this.tools !== undefined && response.toolCalls.length > 0;

      if (!runTools) {
        if (content.trim() === "") {
          emptyRetries += 1;
          if (emptyRetries < MAX_EMPTY_RETRIES) continue;
          const [finContent, finReasoning] = await this.finalizeEmptyResponse(context, key, usage, emit);
          const [finalText, stopReason] =
            finContent.trim() === "" ? [EMPTY_FINAL_RESPONSE_MESSAGE, "empty_final_response"] : [finContent, "completed"];
          persistAssistant(this.sessions, key, finalText, finReasoning, []);
          this.sessions.save(key, false);
          if (this.memory !== undefined) this.memory.appendHistory(finalText, key);
          emit({ type: "final_response", content: finalText });
          return { finalContent: finalText, reasoning: finReasoning, progress, stopReason, usage };
        }

        persistAssistant(this.sessions, key, content, reasoning, []);
        this.sessions.save(key, false);
        if (this.memory !== undefined) this.memory.appendHistory(content, key);
        emit({ type: "final_response", content });
        return { finalContent: content, reasoning, progress, stopReason: "completed", usage };
      }

      const toolCalls = response.toolCalls;
      persistAssistant(this.sessions, key, content, reasoning, toolCalls);
      for (const call of toolCalls) {
        emit({ type: "tool_invoked", name: call.name });
      }
      const results: Array<[string, string]> = toolCalls.map((call) => [
        call.id,
        executeTool(this.tools!, call),
      ]);
      for (const [toolCallId, result] of results) {
        persistToolResult(this.sessions, key, toolCallId, result);
      }
      finalContent = content;
      finalReasoning = reasoning;
    }

    this.sessions.save(key, false);
    if (this.memory !== undefined) this.memory.appendHistory(finalContent, key);
    emit({ type: "final_response", content: finalContent });
    return { finalContent, reasoning: finalReasoning, progress, stopReason: "max_iterations", usage };
  }

  private async finalizeEmptyResponse(
    context: ContextBuilder,
    key: string,
    usage: Record<string, number>,
    emit: (e: ProgressEvent) => void,
  ): Promise<[string, string | undefined]> {
    emit({ type: "finalizing" });
    const history = this.sessions.getOrCreate(key).getHistory(0);
    const messages = [...context.build(history), { role: "user", content: FINALIZATION_RETRY_PROMPT }];
    const runner = new AgentRunner(this.provider, this.settingsValue()).withTools(
      this.tools?.getDefinitions() ?? [],
    );
    const response = await runner
      .runStreaming(this.modelName(), messages, (chunk) => {
        if (chunk.reasoningDelta) emit({ type: "reasoning_delta", text: chunk.reasoningDelta });
        if (chunk.contentDelta) emit({ type: "content_delta", text: chunk.contentDelta });
      })
      .catch((e) => {
        throw new AgentError("provider", String(e));
      });
    accumulateUsage(usage, response.usage);
    const content = response.content ?? "";
    const reasoning =
      response.reasoningContent !== undefined && response.reasoningContent !== ""
        ? response.reasoningContent
        : undefined;
    return [content, reasoning];
  }
}

function accumulateUsage(target: Record<string, number>, addition: Record<string, Json>): void {
  for (const [k, v] of Object.entries(addition)) {
    if (typeof v === "number" && Number.isInteger(v)) {
      target[k] = (target[k] ?? 0) + v;
    }
  }
}

function persistAssistant(
  sessions: SessionManager,
  key: string,
  content: string,
  reasoning: string | undefined,
  toolCalls: ToolCall[],
): void {
  const extra: Record<string, Json> = {};
  if (reasoning !== undefined) extra["reasoning_content"] = reasoning;
  if (toolCalls.length > 0) extra["tool_calls"] = toolCallsToJson(toolCalls);
  sessions.getOrCreate(key).addMessageWith("assistant", content, extra);
}

function persistToolResult(sessions: SessionManager, key: string, toolCallId: string, content: string): void {
  sessions.getOrCreate(key).addMessageWith("tool", content, { tool_call_id: toolCallId });
}

function executeTool(registry: ToolRegistry, call: ToolCall): string {
  let args: Json;
  if (call.arguments.trim() === "") {
    args = {};
  } else {
    try {
      args = JSON.parse(call.arguments);
    } catch (e) {
      return `工具 '${call.name}' 参数不是合法 JSON: ${e}`;
    }
  }
  const result = registry.execute(call.name, args);
  const content = result.isOk() ? result.value.content : result.error.message;
  return ensureNonemptyToolResult(call.name, content);
}

function ensureNonemptyToolResult(toolName: string, content: string): string {
  return content.trim() === "" ? `(${toolName} completed with no output)` : content;
}

function toolCallsToJson(toolCalls: ToolCall[]): Json[] {
  return toolCalls.map((call) => ({
    id: call.id,
    type: "function",
    function: { name: call.name, arguments: call.arguments },
  }));
}
