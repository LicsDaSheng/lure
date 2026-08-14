//! HTTP 传输抽象（对齐 crates/lure-core/src/provider/http.rs）。

import type { Json } from "./types.js";

export interface HttpRequest {
  url: string;
  headers: [string, string][];
  body: Json;
}

export interface HttpResponse {
  status: number;
  body: string;
}

/// HTTP 传输契约。传输/网络层失败以异常抛出，由上层包装为 `ProviderError::transport`。
export interface HttpTransport {
  postJson(request: HttpRequest): Promise<HttpResponse>;
  postJsonStreaming(request: HttpRequest, onLine: (line: string) => void): Promise<number>;
}
