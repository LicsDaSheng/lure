import { afterEach, describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { WebSocket } from "ws";
import { runBackend, type RunningBackend } from "./main.js";

function tempRoot(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-backend-"));
}

let active: RunningBackend | undefined;

afterEach(() => {
  active?.close();
  active = undefined;
});

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
        if (queue.length > 0) resolve(queue.shift());
        else waiters.push(resolve);
      }),
  };
}

describe("desktop/backend 端到端", () => {
  it("headless server runs echo loop over WS", async () => {
    const root = tempRoot();
    active = await runBackend({ root, model: "echo" });

    const issued = active.issuer.issue();
    const { ws, open, next } = makeClient(`${active.url}/ws?token=${issued.token}`);
    await open();

    expect(await next()).toMatchObject({ event: "status", status: "connected" });

    ws.send(JSON.stringify({ chat_id: "c1", content: "你好" }));
    expect(await next()).toMatchObject({ event: "delta", chat_id: "c1", text: "echo: 你好" });
    expect(await next()).toMatchObject({ event: "message", chat_id: "c1", text: "echo: 你好" });

    ws.close();
  });
});
