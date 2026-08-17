/* global process */
import readline from "node:readline";

const input = readline.createInterface({ input: process.stdin, terminal: false });
input.on("line", (line) => {
  const message = JSON.parse(line);
  if (message.method === "initialize") {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, result: { protocolVersion: "2025-11-25", capabilities: { tools: {} }, serverInfo: { name: "HAWK fixture", version: "1.0.0" } } })}\n`);
  }
  if (message.method === "tools/list") {
    process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id: message.id, result: { tools: [{ name: "fixture.echo", description: "Echo a bounded value", inputSchema: { type: "object", properties: { value: { type: "string" } }, required: ["value"] } }] } })}\n`);
  }
});
