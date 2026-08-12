# Privacy boundaries

The gateway only processes requests routed through its local Base URL. It cannot detect content sent directly to an upstream provider, via a browser page, a hard-coded endpoint, or an unsupported protocol.

For supported routes, the gateway distinguishes inspected text/tool fields from opaque content blocks. Strict mode rejects images, files, audio, documents, unknown content blocks, and other potentially data-bearing structures that the current engine cannot inspect. Balanced and monitor modes may forward those structures, but the desktop activity view identifies the request as partially inspected and lists only a bounded content-type label, never its payload.

Supported local routes are `GET /healthz`, `GET /v1/models`, `POST /v1/chat/completions`, `POST /v1/responses`, `POST /v1/messages`, and `POST /v1/messages/count_tokens`. OpenAI-compatible upstream Base URLs are used exactly as configured: a provider whose documented base ends in `/v1` keeps that prefix, while a provider such as DeepSeek may use a root base URL.

Strict policy fails closed for detected credentials. Other supported text is transformed only in documented protocol fields. Unknown JSON fields and signed Anthropic thinking blocks are not inspected or rebuilt. Detection is heuristic plus deterministic validation; users must review high-risk material.

The current deterministic detector set covers recognized credential formats, Chinese resident IDs and unified social credit codes with validation, Luhn-valid bank cards, mainland Chinese mobile numbers, email addresses, IP/MAC addresses, internal hostnames, bounded coordinate pairs, and per-connection exact custom terms. Custom terms are case-sensitive. Strict pseudonymizes non-credential matches. Balanced masks non-credential matches except IP/MAC/hostname/coordinate identifiers, which it monitors. Monitor reports matches without changing ordinary content. All three policies block recognized credentials.

Request headers are forwarded through a narrow allowlist for provider version, beta, project, organization, idempotency, and content-negotiation fields. Local authentication, cookies, connection-control fields, and caller-supplied content lengths are not forwarded. Response content type, request identifiers, retry guidance, and rate-limit metadata are retained; cookies and body-length/encoding headers are not.

Environment import enumerates only hard-coded, supported variable names. Raw values are held in the desktop backend, represented in the UI by a masked preview, and bound to the protocol and upstream URL that were discovered with them. Editing either field clears the import selection, and the backend rejects a mismatched discovery identifier.

Token monitoring stores only profile ID, model name, timestamp, and provider-reported input, output, total, and cached counts for seven days. It does not estimate missing usage or persist message bodies. Quota lookup is user initiated and is restricted to exact, verified official hosts; an imported or manually entered key is never sent to an arbitrary quota URL. The UI distinguishes account balances from per-key spending limits when the provider contract exposes that distinction.
