//! WebUI token 签发与校验（对齐 crates/lure-core/src/webui/tokens.rs）。

import { randomUUID } from "node:crypto";

export interface IssuedTokens {
  token: string;
  apiToken: string;
}

/// token 签发器：TTL + 容量上限。
export class TokenIssuer {
  private readonly ttlMs: number;
  private readonly maxTokens: number;
  private readonly wsTokens = new Map<string, number>();
  private readonly apiTokens = new Map<string, number>();

  constructor(ttlSecs: number, maxTokens: number) {
    this.ttlMs = ttlSecs * 1000;
    this.maxTokens = maxTokens;
  }

  /// 签发一对 token；容量满时抛错（调用方应先 `tryIssue` 确认）。
  issue(): IssuedTokens {
    const issued = this.tryIssue();
    if (issued === undefined) {
      throw new Error("token 签发容量已满");
    }
    return issued;
  }

  /// 尝试签发一对 token；任一类别在册已满返回 `undefined`。
  tryIssue(): IssuedTokens | undefined {
    this.purgeExpired();
    if (this.wsTokens.size >= this.maxTokens || this.apiTokens.size >= this.maxTokens) {
      return undefined;
    }
    const expiry = Date.now() + this.ttlMs;
    const issued = { token: randomToken(), apiToken: randomToken() };
    this.wsTokens.set(issued.token, expiry);
    this.apiTokens.set(issued.apiToken, expiry);
    return issued;
  }

  checkWsToken(token: string): boolean {
    return checkMap(this.wsTokens, token);
  }

  checkApiToken(token: string): boolean {
    return checkMap(this.apiTokens, token);
  }

  private purgeExpired(): void {
    const now = Date.now();
    for (const [k, expiry] of this.wsTokens) {
      if (expiry <= now) this.wsTokens.delete(k);
    }
    for (const [k, expiry] of this.apiTokens) {
      if (expiry <= now) this.apiTokens.delete(k);
    }
  }
}

function checkMap(tokens: Map<string, number>, token: string): boolean {
  const now = Date.now();
  for (const [k, expiry] of tokens) {
    if (expiry <= now) tokens.delete(k);
  }
  return tokens.has(token);
}

/// `secrets.token_urlsafe` 的等价物：两段 uuid4 拼成 64 字符十六进制。
function randomToken(): string {
  return randomUUID().replace(/-/g, "") + randomUUID().replace(/-/g, "");
}
