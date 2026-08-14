//! WebUI 传输层（Hono HTTP + WS）：token 握手 + 把入站消息路由给聊天处理器。

import { Hono } from "hono";
import { createNodeWebSocket } from "@hono/node-ws";
import type { Server } from "node:http";
import {
  errorEvent,
  parseWsInbound,
  statusEvent,
  type Json,
  type TokenIssuer,
} from "@lure/core";

/// 聊天处理器：收到一条入站消息，处理并把出站事件经 `send` 回传。
export type ChatHandler = (
  chatId: string,
  content: string,
  send: (event: Json) => void,
) => void | Promise<void>;

export interface LureApp {
  app: Hono;
  injectWebSocket: (server: Server) => void;
}

export function createApp(issuer: TokenIssuer, onChat: ChatHandler): LureApp {
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
          Promise.resolve(
            onChat(parsed.chatId, parsed.content, (event) => ws.send(JSON.stringify(event))),
          ).catch((e) => {
            ws.send(JSON.stringify(errorEvent(String(e))));
          });
        },
      };
    }),
  );

  return { app, injectWebSocket };
}
