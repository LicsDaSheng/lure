//! 单次 provider 调用的 `AgentRunner`（对齐 crates/lure-core/src/agent/runner.rs）。

import type {
  CompletionRequest,
  GenerationSettings,
  Json,
  LlmProvider,
  LlmResponse,
  StreamChunk,
} from "../provider/types.js";

export class AgentRunner {
  private tools: Json[] = [];

  constructor(
    private readonly provider: LlmProvider,
    private readonly settings: GenerationSettings,
  ) {}

  withTools(tools: Json[]): this {
    this.tools = tools;
    return this;
  }

  async run(model: string, messages: Json[]): Promise<LlmResponse> {
    const request: CompletionRequest = { model, messages, settings: this.settings, tools: this.tools };
    return this.provider.complete(request);
  }

  async runStreaming(
    model: string,
    messages: Json[],
    onDelta: (chunk: StreamChunk) => void,
  ): Promise<LlmResponse> {
    const request: CompletionRequest = { model, messages, settings: this.settings, tools: this.tools };
    return this.provider.completeStreaming(request, onDelta);
  }
}
