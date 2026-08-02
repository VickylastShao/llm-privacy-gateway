import { randomBytes } from "node:crypto";
import { once } from "node:events";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const supportedProtocols = new Set(["openai", "anthropic"]);

function requiredEnvironment(name) {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`missing ${name}`);
  return value;
}

export function nonStreamingText(protocol, body) {
  if (protocol === "openai") return body.choices?.[0]?.message?.content ?? "";
  return Array.isArray(body.content)
    ? body.content.filter((part) => part?.type === "text").map((part) => part.text).join("")
    : "";
}

export function streamingText(protocol, payload) {
  const events = payload.split(/\r?\n\r?\n/);
  let text = "";
  for (const event of events) {
    const data = event
      .split(/\r?\n/)
      .filter((line) => line.startsWith("data:"))
      .map((line) => line.slice(5).trimStart())
      .join("\n");
    if (!data || data === "[DONE]") continue;
    const value = JSON.parse(data);
    if (protocol === "openai") text += value.choices?.[0]?.delta?.content ?? "";
    if (protocol === "anthropic" && value.type === "content_block_delta") {
      text += value.delta?.type === "text_delta" ? value.delta.text ?? "" : "";
    }
  }
  return text;
}

async function waitForHealth(baseUrl, child) {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error("local gateway exited during startup");
    try {
      const response = await fetch(`${baseUrl}/healthz`, { signal: AbortSignal.timeout(1_000) });
      if (response.status === 204) return;
    } catch {
      // Cargo may still be compiling the local development gateway.
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 250));
  }
  throw new Error("local gateway did not become ready within 60 seconds");
}

function requestFor(protocol, baseUrl, localKey, model, canary, stream) {
  const instruction = `Return exactly this email address and nothing else: ${canary}`;
  if (protocol === "openai") {
    return {
      url: `${baseUrl}/v1/chat/completions`,
      init: {
        method: "POST",
        headers: {
          authorization: `Bearer ${localKey}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({
          model,
          messages: [{ role: "user", content: instruction }],
          stream,
          temperature: 0,
        }),
        signal: AbortSignal.timeout(90_000),
      },
    };
  }
  return {
    url: `${baseUrl}/v1/messages`,
    init: {
      method: "POST",
      headers: {
        "x-api-key": localKey,
        "anthropic-version": "2023-06-01",
        "content-type": "application/json",
      },
      body: JSON.stringify({
        model,
        messages: [{ role: "user", content: instruction }],
        max_tokens: 64,
        stream,
        temperature: 0,
      }),
      signal: AbortSignal.timeout(90_000),
    },
  };
}

function assertRestored(text, canary, mode) {
  if (!text.includes(canary)) throw new Error(`${mode} response did not restore the synthetic canary`);
  if (text.includes("__LPG_")) throw new Error(`${mode} response exposed an internal pseudonym marker`);
}

async function main() {
  const protocol = requiredEnvironment("LPG_REAL_PROTOCOL").toLowerCase();
  if (!supportedProtocols.has(protocol)) throw new Error("LPG_REAL_PROTOCOL must be openai or anthropic");
  const upstreamBaseUrl = requiredEnvironment("LPG_REAL_UPSTREAM_BASE_URL");
  requiredEnvironment("LPG_REAL_UPSTREAM_API_KEY");
  const model = requiredEnvironment("LPG_REAL_MODEL");
  const port = Number.parseInt(process.env.LPG_REAL_PORT ?? "17893", 10);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error("invalid LPG_REAL_PORT");

  const baseUrl = `http://127.0.0.1:${port}`;
  const localKey = `lpg_smoke_${randomBytes(18).toString("hex")}`;
  const mappingKey = randomBytes(32).toString("hex");
  const canary = `lpg-smoke-${randomBytes(8).toString("hex")}@example.com`;
  const environment = {
    ...process.env,
    LPG_SMOKE_LOCAL_KEY: localKey,
    LPG_SMOKE_MAPPING_KEY: mappingKey,
  };
  const child = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "-p",
      "lpg-cli",
      "--",
      "serve",
      "--bind",
      `127.0.0.1:${port}`,
      "--protocol",
      protocol,
      "--upstream-base-url",
      upstreamBaseUrl,
      "--upstream-key-env",
      "LPG_REAL_UPSTREAM_API_KEY",
      "--local-key-env",
      "LPG_SMOKE_LOCAL_KEY",
      "--mapping-key-env",
      "LPG_SMOKE_MAPPING_KEY",
    ],
    { cwd: fileURLToPath(new URL("..", import.meta.url)), env: environment, stdio: "ignore" },
  );

  const started = Date.now();
  try {
    await waitForHealth(baseUrl, child);

    const ordinaryRequest = requestFor(protocol, baseUrl, localKey, model, canary, false);
    const ordinaryResponse = await fetch(ordinaryRequest.url, ordinaryRequest.init);
    if (!ordinaryResponse.ok) throw new Error(`non-streaming request returned HTTP ${ordinaryResponse.status}`);
    assertRestored(nonStreamingText(protocol, await ordinaryResponse.json()), canary, "non-streaming");

    const streamRequest = requestFor(protocol, baseUrl, localKey, model, canary, true);
    const streamResponse = await fetch(streamRequest.url, streamRequest.init);
    if (!streamResponse.ok) throw new Error(`streaming request returned HTTP ${streamResponse.status}`);
    assertRestored(streamingText(protocol, await streamResponse.text()), canary, "streaming");

    console.log(
      JSON.stringify({
        status: "PASS",
        protocol,
        model,
        nonStreamingRestoration: true,
        streamingRestoration: true,
        elapsedMs: Date.now() - started,
      }),
    );
  } finally {
    if (child.exitCode === null) child.kill();
    await Promise.race([once(child, "exit"), new Promise((resolvePromise) => setTimeout(resolvePromise, 5_000))]);
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => {
    console.error(`Real-provider smoke failed: ${error.message}`);
    process.exitCode = 1;
  });
}
