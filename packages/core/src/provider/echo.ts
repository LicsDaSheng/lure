//! `EchoProvider`：回显最近一条 user 消息内容（对齐 crates/lure-core/src/provider/echo.rs）。

import type { CompletionRequest, LlmResponse } from "./types.js";
import { asObject, asString } from "./json.js";

export class EchoProvider {
  private readonly model = "echo";

  defaultModel(): string {
    return this.model;
  }

  async complete(request: CompletionRequest): Promise<LlmResponse> {
    let lastUser = "";
    for (let i = request.messages.length - 1; i >= 0; i--) {
      const m = asObject(request.messages[i]);
      if (asString(m?.["role"]) === "user") {
        lastUser = asString(m?.["content"]) ?? "";
        break;
      }
    }
    return { content: `echo: ${lastUser}`, finishReason: "stop", usage: {}, toolCalls: [] };
  }

  async completeStreaming(
    request: CompletionRequest,
    onDelta: (chunk: import("./types.js").StreamChunk) => void,
  ): Promise<LlmResponse> {
    const response = await this.complete(request);
    if (response.content) {
      onDelta({
        contentDelta: response.content,
        finishReason: response.finishReason,
        toolCallDeltas: [],
        usage: {},
      });
    }
    return response;
  }
}
