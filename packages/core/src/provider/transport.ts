//! 基于 `fetch` 的异步 HTTP 传输（对齐 crates/lure-core/src/provider/transport.rs 的 ReqwestTransport）。

import type { HttpRequest, HttpResponse, HttpTransport } from "./http.js";

const DEFAULT_TIMEOUT_MS = 120_000;

export class FetchTransport implements HttpTransport {
  async postJson(request: HttpRequest): Promise<HttpResponse> {
    const response = await fetch(request.url, {
      method: "POST",
      headers: Object.fromEntries(request.headers),
      body: JSON.stringify(request.body),
      signal: AbortSignal.timeout(DEFAULT_TIMEOUT_MS),
    });
    return { status: response.status, body: await response.text() };
  }

  async postJsonStreaming(request: HttpRequest, onLine: (line: string) => void): Promise<number> {
    const response = await fetch(request.url, {
      method: "POST",
      headers: Object.fromEntries(request.headers),
      body: JSON.stringify(request.body),
      signal: AbortSignal.timeout(DEFAULT_TIMEOUT_MS),
    });
    const status = response.status;
    const stream = response.body;
    if (stream === null) return status;

    const decoder = new TextDecoder();
    let pending = "";
    for await (const chunk of stream) {
      pending += decoder.decode(chunk, { stream: true });
      let idx;
      while ((idx = pending.indexOf("\n")) >= 0) {
        onLine(pending.slice(0, idx));
        pending = pending.slice(idx + 1);
      }
    }
    pending += decoder.decode();
    if (pending.length > 0) onLine(pending);
    return status;
  }
}
