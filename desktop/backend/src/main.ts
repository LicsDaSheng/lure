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
  MemoryStore,
  messageEvent,
  registryFromConfig,
  SessionManager,
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

  const sessions = SessionManager.forWorkspace(workspace);
  const tools = registryFromConfig(config.value, workspace);
  if (tools.isErr()) throw tools.error;
  const memory = new MemoryStore(workspace);
  const context = ContextBuilder.forWorkspace(workspace);
  const loop = new AgentLoop(provider.value, sessions, context).withTools(tools.value).withMemory(memory);

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

export function parseArgs(argv: string[]): BackendOptions {
  const opts: BackendOptions = {};
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i]!;
    if (flag === "--" || flag === "--headless") continue;
    const value = argv[++i];
    if (value === undefined) throw new Error(`${flag} 缺参数值`);
    if (flag === "--config") opts.config = value;
    else if (flag === "--model") opts.model = value;
    else if (flag === "--workspace") opts.workspace = value;
    else if (flag === "--root") opts.root = value;
    else if (flag === "--http-port") opts.httpPort = Number(value);
    else throw new Error(`未知参数: ${flag}`);
  }
  return opts;
}

export async function main(argv: string[]): Promise<void> {
  const backend = await runBackend(parseArgs(argv));
  console.log(`LURE_HTTP_URL=${backend.url}`);
  console.log(`LURE_WS_URL=${backend.url.replace("http", "ws")}/ws`);
  // headless server 常驻：保持进程存活。
  await new Promise(() => {});
}
