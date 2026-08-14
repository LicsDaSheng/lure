//! 最小 agent loop（一次性对话）。session / memory / tool-call 循环留待 Phase 2/3。

import type { Json, LlmProvider } from "../provider/types.js";
import { defaultGenerationSettings } from "../provider/types.js";
import type { LlmRuntime } from "../provider/runtime.js";
import { AgentRunner } from "./runner.js";
import type { ContextBuilder } from "./context.js";

export interface InboundMessage {
  channel: string;
  chatId: string;
  content: string;
}

export type ProgressEvent =
  | { type: "turn_started"; sessionKey: string }
  | { type: "content_delta"; text: string }
  | { type: "reasoning_delta"; text: string }
  | { type: "final_response"; content: string };

export interface TurnOutcome {
  finalContent: string;
  reasoning?: string;
  progress: ProgressEvent[];
  stopReason: string;
}

export class AgentLoop {
  private model?: string;
  private settings?: ReturnType<typeof defaultGenerationSettings>;

  constructor(
    private readonly provider: LlmProvider,
    private readonly context: ContextBuilder,
  ) {}

  withRuntime(runtime: LlmRuntime): this {
    this.model = runtime.provider.model;
    this.settings = runtime.settings;
    return this;
  }

  /// 处理一条 inbound 消息，跑完一次性闭环并返回产出。
  async run(input: InboundMessage, onProgress?: (e: ProgressEvent) => void): Promise<TurnOutcome> {
    const model = this.model ?? this.provider.defaultModel();
    const settings = this.settings ?? defaultGenerationSettings();
    const progress: ProgressEvent[] = [];
    const emit = (e: ProgressEvent) => {
      onProgress?.(e);
      progress.push(e);
    };

    emit({ type: "turn_started", sessionKey: input.chatId });

    const history: Json[] = [{ role: "user", content: input.content }];
    const messages = this.context.build(history);
    const runner = new AgentRunner(this.provider, settings);
    const response = await runner.runStreaming(model, messages, (chunk) => {
      if (chunk.reasoningDelta) emit({ type: "reasoning_delta", text: chunk.reasoningDelta });
      if (chunk.contentDelta) emit({ type: "content_delta", text: chunk.contentDelta });
    });

    const content = response.content ?? "";
    emit({ type: "final_response", content });
    return {
      finalContent: content,
      reasoning: response.reasoningContent,
      progress,
      stopReason: "completed",
    };
  }
}
