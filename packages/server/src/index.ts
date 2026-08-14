//! WebUI 传输层（Hono HTTP + WS）。spike：验证 token 握手 + 复用协议。

import { Hono } from "hono";
import { createNodeWebSocket } from "@hono/node-ws";
import type { Server } from "node:http";
import {
  deltaEvent,
  errorEvent,
  messageEvent,
  parseWsInbound,
  statusEvent,
  type TokenIssuer,
} from "@lure/core";

export interface LureApp {
  app: Hono;
  injectWebSocket: (server: Server) => void;
}

/// 构建 Hono 应用：`/ws` 带 token 握手（中间件校验）与复用协议事件回送。
export function createApp(issuer: TokenIssuer): LureApp {
  const app = new Hono();
  const { injectWebSocket, upgradeWebSocket } = createNodeWebSocket({ app });

  // token 校验必须在 upgradeWebSocket 之前：@hono/node-ws 回调返回 Response
  // 不会中止升级，须由中间件在握手前拒绝。
  app.get(
    "/ws",
    async (c, next) => {
      const token = c.req.query("token") ?? "";
      if (!issuer.checkWsToken(token)) {
        return c.json({ error: "unauthorized" }, 401);
      }
      await next();
    },
    upgradeWebSocket((c) => {
      return {
        onOpen(_evt, ws) {
          ws.send(JSON.stringify(statusEvent("connected")));
        },
        onMessage(evt, ws) {
          const parsed = parseWsInbound(JSON.parse(String(evt.data)), "webui");
          if (!parsed.ok) {
            ws.send(JSON.stringify(errorEvent(parsed.detail)));
            return;
          }
          ws.send(JSON.stringify(deltaEvent(parsed.chatId, parsed.content)));
          ws.send(JSON.stringify(messageEvent(parsed.chatId, parsed.content)));
        },
      };
    }),
  );

  return { app, injectWebSocket };
}
