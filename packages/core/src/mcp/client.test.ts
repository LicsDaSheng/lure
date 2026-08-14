import { describe, expect, it } from "vitest";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const mockServerPath = fileURLToPath(new URL("./mock-server.mjs", import.meta.url));

describe("MCP SDK stdio spike", () => {
  it("spawns stdio server, initializes, lists and calls tools", { timeout: 15000 }, async () => {
    const transport = new StdioClientTransport({
      command: process.execPath,
      args: [mockServerPath],
    });
    const client = new Client({ name: "lure-spike", version: "0.0.0" });
    await client.connect(transport);

    const tools = await client.listTools();
    expect(tools.tools.map((t) => t.name)).toContain("echo");

    const result = await client.callTool({ name: "echo", arguments: { text: "hi" } });
    expect(result.content).toEqual([
      { type: "text", text: JSON.stringify({ text: "hi" }) },
    ]);

    await client.close();
  });
});
