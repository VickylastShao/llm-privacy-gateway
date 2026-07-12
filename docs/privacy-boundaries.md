# Privacy boundaries

The gateway only processes requests routed through its local Base URL. It cannot detect content sent directly to an upstream provider, via a browser page, a hard-coded endpoint, or an unsupported protocol.

Supported local routes are `GET /healthz`, `GET /v1/models`, `POST /v1/chat/completions`, `POST /v1/responses`, `POST /v1/messages`, and `POST /v1/messages/count_tokens`. OpenAI-compatible upstream Base URLs are used exactly as configured: a provider whose documented base ends in `/v1` keeps that prefix, while a provider such as DeepSeek may use a root base URL.

Strict policy fails closed for detected credentials. Other supported text is transformed only in documented protocol fields. Unknown JSON fields and signed Anthropic thinking blocks are not inspected or rebuilt. Detection is heuristic plus deterministic validation; users must review high-risk material.
