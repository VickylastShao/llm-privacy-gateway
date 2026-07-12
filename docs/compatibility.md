# Compatibility matrix

Implemented protocol primitives:

| Protocol | JSON request fields | SSE helper |
| --- | --- | --- |
| OpenAI Chat Completions | message text, text blocks, tool arguments | event framing and placeholder restoration |
| OpenAI Responses | instructions, string input, input_text, function output | event framing and placeholder restoration |
| Anthropic Messages | system, text, tool result/input strings | not yet connected to gateway route |

Gateway route forwarding and official SDK end-to-end compatibility tests remain required before a release claim.
