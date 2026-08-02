import assert from "node:assert/strict";
import test from "node:test";
import { nonStreamingText, streamingText } from "../../scripts/real-provider-smoke.mjs";

test("extracts restored text from OpenAI and Anthropic JSON responses", () => {
  assert.equal(
    nonStreamingText("openai", { choices: [{ message: { content: "restored@example.com" } }] }),
    "restored@example.com",
  );
  assert.equal(
    nonStreamingText("anthropic", {
      content: [
        { type: "thinking", thinking: "ignored" },
        { type: "text", text: "restored@example.com" },
      ],
    }),
    "restored@example.com",
  );
});

test("extracts restored text from OpenAI and Anthropic SSE responses", () => {
  const openai = [
    'data: {"choices":[{"delta":{"content":"restored@"}}]}',
    "",
    'data: {"choices":[{"delta":{"content":"example.com"}}]}',
    "",
    "data: [DONE]",
    "",
  ].join("\n");
  assert.equal(streamingText("openai", openai), "restored@example.com");

  const anthropic = [
    "event: content_block_delta",
    'data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"restored@"}}',
    "",
    "event: ping",
    'data: {"type":"ping"}',
    "",
    "event: content_block_delta",
    'data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"example.com"}}',
    "",
  ].join("\r\n");
  assert.equal(streamingText("anthropic", anthropic), "restored@example.com");
});
