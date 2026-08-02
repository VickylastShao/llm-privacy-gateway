import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import http from "node:http";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import OpenAI from "openai";
import Anthropic from "@anthropic-ai/sdk";

const captures = [];
const repositoryRoot = fileURLToPath(new URL("../../../", import.meta.url));
const commandEnvironment = {
  ...process.env,
  PATH: `${process.env.HOME}/.cargo/bin:/usr/bin:/bin:${process.env.PATH}`,
};
let gatewayBuild;

function buildGateway() {
  gatewayBuild ??= new Promise((resolve, reject) => {
    execFile(
      "cargo",
      ["build", "--quiet", "--locked", "-p", "lpg-cli"],
      {
        cwd: repositoryRoot,
        env: commandEnvironment,
        timeout: 120_000,
        maxBuffer: 1024 * 1024,
      },
      (error, _stdout, stderr) => {
        if (error) reject(new Error(`gateway build failed: ${stderr || error.message}`));
        else resolve();
      },
    );
  });
  return gatewayBuild;
}

function gatewayExecutable() {
  const targetDirectory = process.env.CARGO_TARGET_DIR
    ? path.resolve(repositoryRoot, process.env.CARGO_TARGET_DIR)
    : path.join(repositoryRoot, "target");
  return path.join(targetDirectory, "debug", process.platform === "win32" ? "lpg-cli.exe" : "lpg-cli");
}

function listen() {
  return new Promise((resolve) => {
    const server = http.createServer((request, response) => {
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", () => {
        const body = Buffer.concat(chunks).toString("utf8");
        const parsed = JSON.parse(body);
        captures.push({ path: request.url, body });
        if (parsed.model === "force-error") {
          response.writeHead(400, { "content-type": "application/json" });
          response.end(JSON.stringify({ error: { message: "forced mock error", type: "invalid_request_error" } }));
          return;
        }
        if (parsed.stream && request.url === "/chat/completions") {
          const event = { id: "chatcmpl_stream_mock", object: "chat.completion.chunk", created: 1, model: "mock", choices: [{ index: 0, delta: { content: "ok" }, finish_reason: "stop" }] };
          response.writeHead(200, { "content-type": "text/event-stream" });
          response.end(`data: ${JSON.stringify(event)}\n\ndata: [DONE]\n\n`);
          return;
        }
        const payload = request.url === "/v1/messages/count_tokens"
          ? { input_tokens: 1 }
          : request.url === "/v1/messages"
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
  await buildGateway();
  const port = await new Promise((resolve) => { const server = http.createServer(); server.listen(0, "127.0.0.1", () => { const value = server.address().port; server.close(() => resolve(value)); }); });
  const child = spawn(gatewayExecutable(), ["serve", "--protocol", protocol, "--bind", `127.0.0.1:${port}`, "--upstream-base-url", `http://127.0.0.1:${upstreamPort}`], { cwd: repositoryRoot, env: { ...commandEnvironment, LPG_UPSTREAM_API_KEY: "synthetic-upstream-key", LPG_LOCAL_API_KEY: "synthetic-local-key", LPG_MAPPING_KEY_HEX: "22".repeat(32) }, stdio: ["ignore", "ignore", "pipe"] });
  let stderr = "";
  child.stderr.on("data", (chunk) => { stderr += chunk.toString("utf8"); });
  for (let retry = 0; retry < 300; retry += 1) {
    try { await fetch(`http://127.0.0.1:${port}/healthz`); return { process: child, port }; } catch { await new Promise((resolve) => setTimeout(resolve, 50)); }
    if (child.exitCode !== null) break;
  }
  child.kill();
  throw new Error(`gateway did not start${stderr ? `: ${stderr.trim()}` : ""}`);
}

async function stopGateway(child) {
  if (child.exitCode !== null) return;
  await new Promise((resolve) => {
    child.once("exit", resolve);
    child.kill();
    setTimeout(resolve, 2_000).unref();
  });
}

test("OpenAI and Anthropic Node SDKs use the local gateway without leaking canaries", { concurrency: false }, async () => {
  const upstream = await listen();
  try {
    let gateway = await startGateway("openai", upstream.address().port);
    const openai = new OpenAI({ baseURL: `http://127.0.0.1:${gateway.port}/v1`, apiKey: "synthetic-local-key" });
    const response = await openai.chat.completions.create({ model: "mock", messages: [{ role: "user", content: "node-canary@example.test" }] });
    assert.equal(response.choices[0].message.content, "ok");
    assert.ok(!captures.at(-1).body.includes("node-canary@example.test"));
    const stream = await openai.chat.completions.create({ model: "mock", stream: true, messages: [{ role: "user", content: "stream-canary@example.test" }] });
    let streamed = "";
    for await (const chunk of stream) streamed += chunk.choices[0].delta.content ?? "";
    assert.equal(streamed, "ok");
    assert.ok(!captures.at(-1).body.includes("stream-canary@example.test"));
    await openai.chat.completions.create({ model: "mock", messages: [{ role: "user", content: "tool-canary@example.test" }], tools: [{ type: "function", function: { name: "lookup", parameters: { type: "object" } } }] });
    assert.ok(!captures.at(-1).body.includes("tool-canary@example.test"));
    await assert.rejects(() => openai.chat.completions.create({ model: "force-error", messages: [{ role: "user", content: "error-canary@example.test" }] }));
    assert.ok(!captures.at(-1).body.includes("error-canary@example.test"));
    await stopGateway(gateway.process);

    gateway = await startGateway("anthropic", upstream.address().port);
    const anthropic = new Anthropic({ baseURL: `http://127.0.0.1:${gateway.port}`, apiKey: "synthetic-local-key" });
    const message = await anthropic.messages.create({ model: "mock", max_tokens: 8, messages: [{ role: "user", content: "node-canary@example.test" }] });
    assert.equal(message.content[0].text, "ok");
    assert.ok(!captures.at(-1).body.includes("node-canary@example.test"));
    const count = await anthropic.messages.countTokens({ model: "mock", messages: [{ role: "user", content: "count-canary@example.test" }] });
    assert.equal(count.input_tokens, 1);
    assert.ok(!captures.at(-1).body.includes("count-canary@example.test"));
    await stopGateway(gateway.process);
  } finally { await new Promise((resolve) => upstream.close(resolve)); }
});
