import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { EchoProvider } from "../provider/echo.js";
import type { LlmProvider, LlmResponse, StreamChunk } from "../provider/types.js";
import { defaultConfig } from "@lure/schema";
import { ensureWorkspace } from "./onboard.js";
import { ContextBuilder } from "./context.js";
import { AgentLoop } from "./loop.js";
import { SessionManager } from "../session/store.js";
import { registryFromConfig } from "../tool/setup.js";

function tempRoot(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-loop-"));
}

function unwrap<T, E>(r: { isErr(): boolean; value: T; error: E }): T {
  if (r.isErr()) throw r.error;
  return r.value;
}

class QueueProvider implements LlmProvider {
  private i = 0;
  constructor(private readonly responses: LlmResponse[]) {}

  defaultModel(): string {
    return "queue";
  }
  async complete(): Promise<LlmResponse> {
    return this.responses[Math.min(this.i++, this.responses.length - 1)]!;
  }
  async completeStreaming(
    _req: Parameters<LlmProvider["completeStreaming"]>[0],
    onDelta: (chunk: StreamChunk) => void,
  ): Promise<LlmResponse> {
    const resp = this.responses[Math.min(this.i++, this.responses.length - 1)]!;
    if (resp.content) {
      onDelta({ contentDelta: resp.content, finishReason: resp.finishReason, toolCallDeltas: [], usage: {} });
    }
    return resp;
  }
}

describe("AgentLoop 完整闭环", () => {
  it("runs one-shot echo loop end-to-end", async () => {
    const root = tempRoot();
    const { workspace } = ensureWorkspace(root);
    const sessions = SessionManager.forWorkspace(workspace);
    const loop = new AgentLoop(new EchoProvider(), sessions, ContextBuilder.forWorkspace(workspace));

    const outcome = await loop.run({ channel: "webui", chatId: "c1", content: "你好" });
    expect(outcome.finalContent).toBe("echo: 你好");
    expect(outcome.stopReason).toBe("completed");
  });

  it("executes tool calls across turns", async () => {
    const root = tempRoot();
    const { workspace } = ensureWorkspace(root);
    const sessions = SessionManager.forWorkspace(workspace);
    const tools = unwrap(registryFromConfig(defaultConfig(), workspace));
    const provider = new QueueProvider([
      {
        content: undefined,
        finishReason: "tool_calls",
        usage: {},
        toolCalls: [{ id: "c1", name: "write_file", arguments: JSON.stringify({ path: "out.txt", content: "hi" }) }],
      },
      { content: "done", finishReason: "stop", usage: {}, toolCalls: [] },
    ]);
    const loop = new AgentLoop(provider, sessions, ContextBuilder.forWorkspace(workspace)).withTools(tools);

    const outcome = await loop.run({ channel: "webui", chatId: "c1", content: "write it" });

    expect(outcome.finalContent).toBe("done");
    expect(outcome.stopReason).toBe("completed");
    expect(fs.readFileSync(path.join(workspace, "out.txt"), "utf8")).toBe("hi");
    expect(outcome.progress.some((e) => e.type === "tool_invoked" && e.name === "write_file")).toBe(true);
  });
});
