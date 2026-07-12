# Compatibility matrix

Implemented protocol primitives:

| Protocol | JSON request fields | SSE helper |
| --- | --- | --- |
| OpenAI Chat Completions | message text, text blocks, tool arguments | event framing and placeholder restoration |
| OpenAI Responses | instructions, string input, input_text, function output | event framing and placeholder restoration |
| Anthropic Messages/count_tokens | system, text, tool result/input strings; signed thinking blocks retained as raw JSON | CRLF/LF and multiline event framing; text restoration; buffered `input_json_delta` restoration |

Gateway JSON forwarding is implemented. Mock compatibility coverage pins OpenAI Python 2.45.0, Anthropic Python 0.116.0, OpenAI Node 6.46.0, and Anthropic TypeScript SDK 0.111.0. Real-provider end-to-end compatibility remains required before a release claim.
