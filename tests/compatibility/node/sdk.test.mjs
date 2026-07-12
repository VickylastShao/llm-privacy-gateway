import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import http from "node:http";
import test from "node:test";
import OpenAI from "openai";
import Anthropic from "@anthropic-ai/sdk";

const captures = [];

function listen() {
  return new Promise((resolve) => {
    const server = http.createServer((request, response) => {
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", () => {
        captures.push({ path: request.url, body: Buffer.concat(chunks).toString("utf8") });
        const payload = request.url === "/v1/messages"
          ? { id: "msg_mock", type: "message", role: "assistant", model: "mock", content: [{ type: "text", text: "ok" }], stop_reason: "end_turn", usage: { input_tokens: 1, output_tokens: 1 } }
          : { id: "chatcmpl_mock", object: "chat.completion", created: 1, model: "mock", choices: [{ index: 0, message: { role: "assistant", content: "ok" }, finish_reason: "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } };
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify(payload));
      });
    });
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

async function startGateway(protocol, upstreamPort) {
  const port = await new Promise((resolve) => { const server = http.createServer(); server.listen(0, "127.0.0.1", () => { const value = server.address().port; server.close(() => resolve(value)); }); });
  const child = spawn("cargo", ["run", "--quiet", "-p", "lpg-cli", "--", "serve", "--protocol", protocol, "--bind", `127.0.0.1:${port}`, "--upstream-base-url", `http://127.0.0.1:${upstreamPort}`], { env: { ...process.env, PATH: `${process.env.HOME}/.cargo/bin:/usr/bin:/bin:${process.env.PATH}`, LPG_UPSTREAM_API_KEY: "synthetic-upstream-key", LPG_LOCAL_API_KEY: "synthetic-local-key", LPG_MAPPING_KEY_HEX: "22".repeat(32) } });
  for (let retry = 0; retry < 100; retry += 1) {
    try { await fetch(`http://127.0.0.1:${port}/healthz`); return { process: child, port }; } catch { await new Promise((resolve) => setTimeout(resolve, 50)); }
  }
  child.kill();
  throw new Error("gateway did not start");
}

test("OpenAI and Anthropic Node SDKs use the local gateway without leaking a canary", { concurrency: false }, async () => {
  const upstream = await listen();
  try {
    let gateway = await startGateway("openai", upstream.address().port);
    const openai = new OpenAI({ baseURL: `http://127.0.0.1:${gateway.port}/v1`, apiKey: "synthetic-local-key" });
    const response = await openai.chat.completions.create({ model: "mock", messages: [{ role: "user", content: "node-canary@example.test" }] });
    assert.equal(response.choices[0].message.content, "ok");
    assert.ok(!captures.at(-1).body.includes("node-canary@example.test"));
    gateway.process.kill();

    gateway = await startGateway("anthropic", upstream.address().port);
    const anthropic = new Anthropic({ baseURL: `http://127.0.0.1:${gateway.port}`, apiKey: "synthetic-local-key" });
    const message = await anthropic.messages.create({ model: "mock", max_tokens: 8, messages: [{ role: "user", content: "node-canary@example.test" }] });
    assert.equal(message.content[0].text, "ok");
    assert.ok(!captures.at(-1).body.includes("node-canary@example.test"));
    gateway.process.kill();
  } finally { await new Promise((resolve) => upstream.close(resolve)); }
});
