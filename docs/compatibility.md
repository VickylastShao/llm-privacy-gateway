# Compatibility matrix

Implemented protocol primitives:

| Protocol | JSON request fields | SSE helper |
| --- | --- | --- |
| OpenAI Chat Completions | message text, text blocks, tool arguments | event framing and placeholder restoration |
| OpenAI Responses | instructions, string input, input_text, function output | event framing and placeholder restoration |
| Anthropic Messages/count_tokens | system, text, tool result/input strings; signed thinking blocks retained as raw JSON | CRLF/LF and multiline event framing; text restoration; buffered `input_json_delta` restoration |

Gateway JSON forwarding is implemented. Mock compatibility coverage pins OpenAI Python 2.45.0, Anthropic Python 0.116.0, OpenAI Node 6.46.0, and Anthropic TypeScript SDK 0.111.0. The pinned Python and Node suites exercise OpenAI Chat Completions JSON, SSE, tools, and error propagation, plus Anthropic Messages and `count_tokens`; every request contains a synthetic canary that must be absent from the mock upstream capture.

On 2026-07-12, a local gateway was verified against DeepSeek's documented OpenAI-compatible base URL (`https://api.deepseek.com`) and `deepseek-v4-flash`. The non-streaming and SSE Chat Completions paths restored a synthetic email canary; the SSE assertion also required that no `__LPG_` marker reached the client. This is a narrow manual verification, not a general release-compatibility claim. Anthropic real-provider, Responses API, SDK tool streaming, and error-path verification remain required before release.
