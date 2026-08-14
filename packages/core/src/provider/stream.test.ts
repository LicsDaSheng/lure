import { describe, expect, it } from "vitest";
import {
  OpenAiCompatProvider,
  parseSseLine,
} from "./index.js";
import { defaultGenerationSettings, ProviderError, type CompletionRequest, type Json, type StreamChunk } from "./types.js";
import type { HttpRequest, HttpResponse, HttpTransport } from "./http.js";

class FakeStreamTransport implements HttpTransport {
  constructor(
    private readonly status: number,
    private readonly lines: string[],
  ) {}

  async postJson(_request: HttpRequest): Promise<HttpResponse> {
    return { status: this.status, body: this.lines.join("\n") };
  }

  async postJsonStreaming(_request: HttpRequest, onLine: (line: string) => void): Promise<number> {
    for (const line of this.lines) onLine(line);
    return this.status;
  }
}

class RecordingStreamTransport implements HttpTransport {
  constructor(private readonly seen: { body?: Json }) {}

  async postJson(_request: HttpRequest): Promise<HttpResponse> {
    return { status: 200, body: "" };
  }

  async postJsonStreaming(request: HttpRequest, _onLine: (line: string) => void): Promise<number> {
    this.seen.body = request.body;
    return 200;
  }
}

function request(): CompletionRequest {
  return {
    model: "gpt-4o",
    messages: [{ role: "user", content: "hi" }],
    settings: defaultGenerationSettings(),
    tools: [],
  };
}

describe("parse_sse_line", () => {
  it("extracts content delta", () => {
    const line = `data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}`;
    const chunk = parseSseLine(line);
    expect(chunk?.contentDelta).toBe("Hello");
    expect(chunk?.finishReason).toBeUndefined();
  });

  it("done and non-data yield null", () => {
    expect(parseSseLine("data: [DONE]")).toBeNull();
    expect(parseSseLine(": keep-alive")).toBeNull();
    expect(parseSseLine("")).toBeNull();
  });

  it("reads finish reason", () => {
    const line = `data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`;
    const chunk = parseSseLine(line);
    expect(chunk?.contentDelta).toBeUndefined();
    expect(chunk?.finishReason).toBe("stop");
  });

  it("extracts usage from usage-only chunk", () => {
    const line = `data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}`;
    const chunk = parseSseLine(line);
    expect(chunk?.usage["prompt_tokens"]).toBe(12);
    expect(chunk?.usage["total_tokens"]).toBe(17);
    expect(chunk?.contentDelta).toBeUndefined();
  });
});

describe("complete_streaming", () => {
  it("delivers deltas in order and assembles response", async () => {
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new FakeStreamTransport(200, [
        `data: {"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}`,
        `data: {"choices":[{"delta":{"content":" world"},"finish_reason":null}]}`,
        `data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
        "data: [DONE]",
      ]),
    );

    const deltas: string[] = [];
    const response = await provider.completeStreaming(request(), (chunk: StreamChunk) => {
      if (chunk.contentDelta !== undefined) deltas.push(chunk.contentDelta);
    });

    expect(deltas).toEqual(["Hello", " world"]);
    expect(response.content).toBe("Hello world");
    expect(response.finishReason).toBe("stop");
  });

  it("assembles tool calls across chunks", async () => {
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new FakeStreamTransport(200, [
        `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"echo","arguments":"{\\"te"}}]}}]}`,
        `data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"xt\\":\\"hi\\"}"}}]}}]}`,
        `data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}`,
        "data: [DONE]",
      ]),
    );

    const response = await provider.completeStreaming(request(), () => {});

    expect(response.finishReason).toBe("tool_calls");
    expect(response.toolCalls).toHaveLength(1);
    expect(response.toolCalls[0]?.id).toBe("call_1");
    expect(response.toolCalls[0]?.name).toBe("echo");
    expect(response.toolCalls[0]?.arguments).toBe(`{"text":"hi"}`);
  });

  it("captures usage from final chunk", async () => {
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new FakeStreamTransport(200, [
        `data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}`,
        `data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
        `data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":5,"total_tokens":17}}`,
        "data: [DONE]",
      ]),
    );

    const response = await provider.completeStreaming(request(), () => {});

    expect(response.content).toBe("Hi");
    expect(response.finishReason).toBe("stop");
    expect(response.usage["prompt_tokens"]).toBe(12);
    expect(response.usage["completion_tokens"]).toBe(5);
    expect(response.usage["total_tokens"]).toBe(17);
  });

  it("normalizes nested cached tokens", async () => {
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new FakeStreamTransport(200, [
        `data: {"choices":[{"delta":{"content":"Hi"},"finish_reason":null}]}`,
        `data: {"choices":[{"delta":{},"finish_reason":"stop"}]}`,
        `data: {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":10,"total_tokens":110,"prompt_tokens_details":{"cached_tokens":80}}}`,
        "data: [DONE]",
      ]),
    );

    const response = await provider.completeStreaming(request(), () => {});

    expect(response.usage["cached_tokens"]).toBe(80);
    expect(response.usage["prompt_tokens"]).toBe(100);
  });

  it("streaming request includes stream_options.include_usage", async () => {
    const seen: { body?: Json } = {};
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new RecordingStreamTransport(seen),
    );

    await provider.completeStreaming(request(), () => {});

    const body = seen.body as Record<string, Json>;
    expect(body["stream"]).toBe(true);
    expect((body["stream_options"] as Record<string, Json>)["include_usage"]).toBe(true);
  });

  it("classifies http error status", async () => {
    const provider = new OpenAiCompatProvider(
      "https://api.test/v1",
      undefined,
      "gpt-4o",
      new FakeStreamTransport(401, [`{"error":"bad key"}`]),
    );

    let thrown: ProviderError | undefined;
    try {
      await provider.completeStreaming(request(), () => {});
    } catch (e) {
      thrown = e as ProviderError;
    }

    expect(thrown?.kind).toBe("auth");
    expect(thrown?.status).toBe(401);
  });
});
