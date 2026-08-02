# Compatibility matrix

Implemented protocol primitives:

| Protocol | JSON request fields | SSE helper |
| --- | --- | --- |
| OpenAI Chat Completions | message strings, text blocks, and string leaves inside tool/function arguments | text and per-choice/per-tool argument restoration, including split placeholders |
| OpenAI Responses | instructions, string input, `input_text`, and text inside `function_call_output` | per-output text/argument restoration plus final response, item, and part events |
| Anthropic Messages/count_tokens | system/text blocks, text-form tool results, and nested string leaves inside `tool_use.input`; signed thinking remains raw | CRLF/LF and multiline framing, text/tool-input restoration, and buffered `input_json_delta` restoration |

Gateway JSON forwarding is implemented. Mock compatibility coverage pins OpenAI Python 2.45.0, Anthropic Python 0.116.0, OpenAI Node 6.46.0, and Anthropic TypeScript SDK 0.111.0. The pinned Python and Node suites exercise OpenAI Chat Completions JSON, SSE, tools, and error propagation, plus Anthropic Messages and `count_tokens`; every request contains a synthetic canary that must be absent from the mock upstream capture. Rust protocol tests additionally cover OpenAI Responses, nested tool inputs, unknown-event passthrough, signed-thinking preservation, and placeholders split across stream chunks.

The gateway forwards a documented-safe subset of provider headers: Anthropic version/beta, OpenAI organization/project, idempotency, and Accept on requests; content type, request IDs, retry guidance, and rate-limit metadata on responses. Local authentication and cookies are intentionally not transparent.

On 2026-07-12, a local gateway was verified against DeepSeek's documented OpenAI-compatible base URL (`https://api.deepseek.com`) and `deepseek-v4-flash`. The non-streaming and SSE Chat Completions paths restored a synthetic email canary; the SSE assertion also required that no `__LPG_` marker reached the client. This is a narrow manual verification, not a general release-compatibility claim. Anthropic real-provider, Responses API, SDK tool streaming, and error-path verification remain required before release.

For a repeatable real-provider check, set `LPG_REAL_PROTOCOL`, `LPG_REAL_UPSTREAM_BASE_URL`,
`LPG_REAL_UPSTREAM_API_KEY`, and `LPG_REAL_MODEL`, then run:

```bash
npm run smoke:provider
```

The script starts the development gateway on loopback, sends a generated synthetic email canary
through non-streaming and SSE requests, verifies local restoration, and prints no key or prompt
content. It is evidence of the selected model and route only; it does not generalize to every model
or provider feature.
