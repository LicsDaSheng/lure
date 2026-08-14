import { afterEach, describe, expect, it } from "vitest";
import { serve } from "@hono/node-server";
import type { AddressInfo } from "node:net";
import type { Server } from "node:http";
import { WebSocket } from "ws";
import { TokenIssuer } from "@lure/core";
import { createApp } from "./index.js";

function startServer(): { url: string; server: Server; issuer: TokenIssuer } {
  const issuer = new TokenIssuer(3600, 16);
  const { app, injectWebSocket } = createApp(issuer);
  const server = serve({ fetch: app.fetch, port: 0 });
  injectWebSocket(server);
  const address = server.address() as AddressInfo;
  return { url: `ws://127.0.0.1:${address.port}`, server, issuer };
}

let active: Server | undefined;

afterEach(() => {
  active?.close();
  active = undefined;
});

/// 连接并立即缓冲入站消息，避免「先 open 后挂监听」的竞态。
function makeClient(url: string) {
  const ws = new WebSocket(url);
  const queue: unknown[] = [];
  const waiters: Array<(v: unknown) => void> = [];
  ws.on("message", (data) => {
    const msg = JSON.parse(data.toString());
    const waiter = waiters.shift();
    if (waiter) waiter(msg);
    else queue.push(msg);
  });
  return {
    ws,
    open: () =>
      new Promise<void>((resolve, reject) => {
        ws.on("open", () => resolve());
        ws.on("error", reject);
      }),
    next: () =>
      new Promise<unknown>((resolve) => {
        if (queue.length > 0) {
          resolve(queue.shift());
        } else {
          waiters.push(resolve);
        }
      }),
  };
}

describe("Hono WS spike：token 握手 + 复用协议", () => {
  it("rejects handshake with invalid token", async () => {
    const { url, server } = startServer();
    active = server;

    const { open } = makeClient(`${url}/ws?token=bad`);
    await expect(open()).rejects.toThrow();
  });

  it("accepts valid token and echoes reuse-protocol events", async () => {
    const { url, server, issuer } = startServer();
    active = server;
    const issued = issuer.issue();

    const { ws, open, next } = makeClient(`${url}/ws?token=${issued.token}`);
    await open();

    expect(await next()).toMatchObject({ event: "status", status: "connected" });

    ws.send(JSON.stringify({ chat_id: "c1", content: "你好" }));
    expect(await next()).toMatchObject({ event: "delta", chat_id: "c1", text: "你好" });
    expect(await next()).toMatchObject({ event: "message", chat_id: "c1", text: "你好" });

    ws.close();
  });

  it("emits error event for invalid inbound frame", async () => {
    const { url, server, issuer } = startServer();
    active = server;
    const issued = issuer.issue();

    const { ws, open, next } = makeClient(`${url}/ws?token=${issued.token}`);
    await open();
    await next(); // 消费 status 事件

    ws.send(JSON.stringify({ content: "no chat_id" }));
    expect(await next()).toMatchObject({ event: "error", detail: "invalid chat_id" });

    ws.close();
  });
});
