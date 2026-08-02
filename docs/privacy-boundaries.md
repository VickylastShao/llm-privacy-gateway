# Privacy boundaries

The gateway only processes requests routed through its local Base URL. It cannot detect content sent directly to an upstream provider, via a browser page, a hard-coded endpoint, or an unsupported protocol.

Supported local routes are `GET /healthz`, `GET /v1/models`, `POST /v1/chat/completions`, `POST /v1/responses`, `POST /v1/messages`, and `POST /v1/messages/count_tokens`. OpenAI-compatible upstream Base URLs are used exactly as configured: a provider whose documented base ends in `/v1` keeps that prefix, while a provider such as DeepSeek may use a root base URL.

Strict policy fails closed for detected credentials. Other supported text is transformed only in documented protocol fields. Unknown JSON fields and signed Anthropic thinking blocks are not inspected or rebuilt. Detection is heuristic plus deterministic validation; users must review high-risk material.

The current deterministic detector set covers recognized credential formats, Chinese resident IDs and unified social credit codes with validation, Luhn-valid bank cards, mainland Chinese mobile numbers, email addresses, IP/MAC addresses, internal hostnames, bounded coordinate pairs, and per-connection exact custom terms. Custom terms are case-sensitive. Strict pseudonymizes non-credential matches. Balanced masks non-credential matches except IP/MAC/hostname/coordinate identifiers, which it monitors. Monitor reports matches without changing ordinary content. All three policies block recognized credentials.

Request headers are forwarded through a narrow allowlist for provider version, beta, project, organization, idempotency, and content-negotiation fields. Local authentication, cookies, connection-control fields, and caller-supplied content lengths are not forwarded. Response content type, request identifiers, retry guidance, and rate-limit metadata are retained; cookies and body-length/encoding headers are not.
