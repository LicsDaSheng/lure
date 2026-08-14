//! desktop/backend 入口：parse flags → ensureWorkspace → buildProvider → AgentLoop → headless server。

import path from "node:path";
import type { Server } from "node:http";
import { serve } from "@hono/node-server";
import {
  AgentLoop,
  buildProvider,
  ContextBuilder,
  defaultLureRoot,
  deltaEvent,
  ensureWorkspace,
  loadConfig,
  messageEvent,
  TokenIssuer,
} from "@lure/core";
import { createApp, type ChatHandler } from "@lure/server";

export interface BackendOptions {
  config?: string;
  model?: string;
  workspace?: string;
  root?: string;
  httpPort?: number;
}

export interface RunningBackend {
  url: string;
  server: Server;
  issuer: TokenIssuer;
  close: () => void;
}

export async function runBackend(opts: BackendOptions): Promise<RunningBackend> {
  const root = opts.root ?? defaultLureRoot();
  ensureWorkspace(root);
  const workspace = opts.workspace ?? path.join(root, "workspace");

  const config = loadConfig(opts.config ?? path.join(root, "config.json"));
  if (config.isErr()) throw config.error;

  const provider = buildProvider(config.value, undefined, opts.model);
  if (provider.isErr()) throw provider.error;

  const context = ContextBuilder.forWorkspace(workspace);
  const loop = new AgentLoop(provider.value, context);

  const issuer = new TokenIssuer(3600, 16);
  const onChat: ChatHandler = async (chatId, content, send) => {
    await loop.run({ channel: "webui", chatId, content }, (e) => {
      if (e.type === "content_delta") send(deltaEvent(chatId, e.text));
      else if (e.type === "final_response") send(messageEvent(chatId, e.content));
    });
  };
  const { app, injectWebSocket } = createApp(issuer, onChat);

  const server = serve({ fetch: app.fetch, port: opts.httpPort ?? 0 });
  injectWebSocket(server);
  const address = server.address() as { port: number };
  const url = `http://127.0.0.1:${address.port}`;

  return { url, server, issuer, close: () => server.close() };
}
