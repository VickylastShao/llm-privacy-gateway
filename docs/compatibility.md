# Compatibility matrix

Implemented protocol primitives:

| Protocol | JSON request fields | SSE helper |
| --- | --- | --- |
| OpenAI Chat Completions | message text, text blocks, tool arguments | event framing and placeholder restoration |
| OpenAI Responses | instructions, string input, input_text, function output | event framing and placeholder restoration |
| Anthropic Messages/count_tokens | system, text, tool result/input strings | request JSON connected; stream handling pending |

Gateway JSON forwarding is implemented. Official SDK and end-to-end stream compatibility tests remain required before a release claim.
