import { describe, expect, it } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { EchoProvider } from "../provider/echo.js";
import { ensureWorkspace } from "./onboard.js";
import { ContextBuilder } from "./context.js";
import { AgentLoop } from "./loop.js";

function tempRoot(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "lure-loop-"));
}

describe("AgentLoop 最小闭环", () => {
  it("runs one-shot echo loop end-to-end", async () => {
    const root = tempRoot();
    const { workspace } = ensureWorkspace(root);

    const context = ContextBuilder.forWorkspace(workspace);
    const loop = new AgentLoop(new EchoProvider(), context);

    const events: string[] = [];
    const outcome = await loop.run({ channel: "webui", chatId: "c1", content: "你好" }, (e) => {
      events.push(e.type);
    });

    expect(outcome.finalContent).toBe("echo: 你好");
    expect(outcome.stopReason).toBe("completed");
    expect(events).toContain("turn_started");
    expect(events).toContain("final_response");
  });
});
