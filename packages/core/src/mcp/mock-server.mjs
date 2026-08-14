// 最小 MCP stdio 服务器（spike 用）：newline-delimited JSON-RPC。
let buf = "";
process.stdin.on("data", (c) => {
  buf += c.toString();
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i).trim();
    buf = buf.slice(i + 1);
    if (!line) continue;
    const req = JSON.parse(line);
    if (req.id === undefined) continue; // 通知无需响应
    let result;
    if (req.method === "initialize") {
      result = {
        protocolVersion: req.params.protocolVersion,
        capabilities: { tools: {} },
        serverInfo: { name: "mock", version: "1.0.0" },
      };
    } else if (req.method === "tools/list") {
      result = {
        tools: [
          { name: "echo", description: "Echo", inputSchema: { type: "object", properties: { text: { type: "string" } } } },
        ],
      };
    } else if (req.method === "tools/call") {
      result = { content: [{ type: "text", text: JSON.stringify(req.params.arguments) }] };
    } else {
      result = {};
    }
    process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: req.id, result }) + "\n");
  }
});
