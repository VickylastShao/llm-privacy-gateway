# Local LLM Privacy Gateway Design

**Date:** 2026-07-12  
**Status:** Approved design, pending written-spec review  
**Target:** Windows-first MVP  
**Confidence:** High for architecture and protocol scope; medium for real-world privacy recall until an in-domain evaluation corpus exists

## 1. Product definition

Build a Windows-first, local-first desktop application that exposes OpenAI- and Anthropic-compatible local API endpoints. Users configure an upstream base URL and API key, receive a local base URL and local key, and point compatible applications at the local gateway. Before supported request content leaves the device, the gateway detects sensitive information and applies a configured policy: allow, monitor, block, mask, or reversibly pseudonymize. Supported response fields are restored locally before being returned to the client.

The MVP is an API-chain privacy gateway, not a system-wide DLP product. It covers only clients that support a custom OpenAI or Anthropic base URL.

### 1.1 MVP scope

The MVP includes:

- a Windows desktop application;
- a loopback-only local HTTP gateway;
- OpenAI Chat Completions and Responses support;
- Anthropic Messages and token-counting support;
- JSON and server-sent event streaming;
- deterministic secret, identifier, and network detectors;
- user-defined sensitive terms and regular expressions;
- block, mask, monitor, allow, and reversible pseudonymization actions;
- encrypted upstream credentials and pseudonym maps;
- a setup wizard, connection manager, policy editor, test console, and metadata-only activity view;
- an NSIS installer and Windows CI release pipeline.

The MVP excludes:

- document and file sanitization;
- browser extensions;
- MCP proxying;
- team administration;
- enterprise employee monitoring;
- system-wide HTTPS interception;
- OpenAI Realtime/WebSocket support;
- image, audio, and video inspection;
- Files, Batch, and Webhook APIs;
- Azure, Bedrock, and Vertex-specific authentication protocols;
- OpenAI-to-Anthropic protocol conversion;
- automatic recognition of all Chinese names, organizations, and addresses;
- cloud-hosted storage or processing of prompts.

## 2. Strongest objections and design responses

### 2.1 The core proxy idea already exists

Projects such as `jayluxferro/llm-redactor`, LiteLLM, and enterprise AI gateways already implement parts of the request-redaction pipeline. The defensible product is not the abstract proxy mechanism. The differentiators are:

- a two-minute Windows desktop setup;
- explicit OpenAI and native Anthropic compatibility;
- Chinese identifiers and user-defined internal terminology;
- secure local credential handling;
- protocol-aware filtering rather than recursive blind replacement;
- versioned compatibility and leak testing;
- metadata-only observability with no prompt logging.

### 2.2 Detection cannot guarantee privacy

No finite detector can identify every sensitive fact, implicit identity, or novel credential format. The product must state that it reduces disclosure risk for configured and supported categories. It must not claim complete anonymization, legal compliance, or zero leakage.

### 2.3 Pseudonymization changes model behavior

Pseudonyms change token counts, cache keys, tool parameters, and sometimes model utility. The product promises semantic compatibility for supported fields, not byte-for-byte transparency. Type-consistent realistic surrogate values are excluded from the MVP because they can introduce false facts and ambiguous restoration.

### 2.4 A security product becomes a high-value target

The application stores provider credentials and reversible mappings. The attack surface is minimized by binding only to loopback, requiring a local key, denying CORS, using the Windows credential store, excluding request bodies from logs, using a restricted Tauri command surface, and rejecting unknown protocol paths by default.

## 3. Technology selection

### 3.1 Selected stack

- Desktop framework: Tauri 2
- Native core: Rust
- UI: React, TypeScript, and Vite
- Local HTTP service: `axum` on `tokio`
- Upstream HTTP client: `reqwest` with `rustls`
- Serialization: `serde` and `serde_json`
- Error types: `thiserror`
- Local configuration/activity database: SQLite via `rusqlite`
- Secret storage: Windows Credential Manager through a narrowly scoped Rust adapter
- Deterministic matching: `regex` and `aho-corasick`
- Cryptography: HMAC-SHA-256 and AES-256-GCM using established RustCrypto crates
- Structured diagnostics: `tracing`, with mandatory redaction at the logging boundary
- Property testing: `proptest`
- HTTP integration testing: a Rust mock upstream such as `wiremock`
- Packaging: Tauri NSIS installer
- CI: GitHub Actions Windows runners

### 3.2 Why this stack

Tauri uses the operating system WebView instead of bundling Chromium, while Rust provides a memory-safe asynchronous gateway core. The gateway, protocol adapters, privacy engine, and stores remain independent of React and can be tested without a desktop UI.

.NET with WinUI 3 is a valid alternative when Windows-only delivery speed and existing C# expertise dominate. It was not selected because the product's primary complexity is a portable protocol and privacy engine rather than native Windows UI.

Electron was rejected because it adds Chromium and Node maintenance and expands the privileged dependency surface for a product that stores API credentials.

## 4. Repository structure

```text
llm-privacy-gateway/
├── apps/
│   └── desktop/                 # Tauri shell and React control plane
├── crates/
│   ├── gateway/                 # Listener, routing, upstream forwarding, limits
│   ├── protocol-openai/         # Chat Completions, Responses, OpenAI SSE
│   ├── protocol-anthropic/      # Messages, token counting, Anthropic SSE
│   ├── privacy-core/            # Detectors, overlap resolution, policies
│   ├── secret-store/            # Windows Credential Manager adapter
│   ├── session-map/             # Encrypted reversible pseudonym maps
│   └── activity-store/          # Metadata-only SQLite persistence
├── fixtures/
│   ├── openai/
│   └── anthropic/
├── privacy-evals/
├── tests/
│   ├── compatibility/
│   └── security/
├── docs/
│   ├── research/
│   ├── security/
│   └── superpowers/
└── references/                  # Ignored shallow clones, never shipped
```

Each crate has one primary responsibility. Protocol adapters identify mutable fields but do not implement detection. The privacy core detects and transforms text but does not know HTTP. The gateway coordinates the adapters, policy engine, stores, and upstream client.

## 5. Runtime architecture

```text
OpenAI or Anthropic client
            |
            | local base URL + local key
            v
      127.0.0.1:17891
            |
            +-- local-key authentication and profile lookup
            +-- OpenAI protocol adapter
            +-- Anthropic protocol adapter
            +-- privacy detection and policy engine
            +-- encrypted pseudonym map
            +-- HTTP/SSE upstream transport
                          |
                          v
                 configured upstream
```

The Rust Tauri process owns the gateway runtime. Closing the window minimizes the application to the system tray; the gateway remains active. Explicitly exiting the application stops the listener.

### 5.1 Listener

- Bind only to `127.0.0.1`; add `::1` only after a separate IPv6 compatibility test.
- Never bind to `0.0.0.0` or a LAN address in the MVP.
- Use port `17891` by default.
- On collision, report a configuration error; do not silently select a random port.
- Disable CORS by default.
- Apply request-header and request-body size limits.
- Require a valid local capability key for every upstream route.

### 5.2 Base URLs

```text
OpenAI:    http://127.0.0.1:17891/v1
Anthropic: http://127.0.0.1:17891
```

Each connection profile has a distinct local key. The local key identifies the connection profile and upstream credential. Protocol type and request path must agree with the profile; a mismatched protocol is rejected.

### 5.3 Upstream URL rules

- Official endpoints are provided as immutable presets.
- Custom upstreams default to HTTPS.
- Plain HTTP is permitted only for loopback addresses in the MVP.
- URLs containing embedded usernames or passwords are rejected.
- Redirects do not automatically forward authentication headers to a different origin.
- The UI displays the exact destination host before saving or testing a custom upstream.

## 6. Protocol scope

### 6.1 OpenAI

Supported:

- `POST /v1/chat/completions`
- `POST /v1/responses`
- `GET /v1/models`
- normal JSON responses;
- SSE streaming;
- text message content;
- function and tool call data in supported string fields.

### 6.2 Anthropic

Supported:

- `POST /v1/messages`
- `POST /v1/messages/count_tokens`
- `GET /v1/models`
- normal JSON responses;
- SSE streaming;
- system and message text;
- supported tool input and tool-result string fields.

### 6.3 Protocol-aware mutation

Adapters return a list of typed, mutable text locations. They must never modify:

- response, message, or tool-call IDs;
- model names and numeric API parameters;
- JSON Schema structure;
- thinking or reasoning signatures;
- encrypted content;
- binary or Base64 content;
- event names, sequence numbers, or protocol-control fields;
- unknown content block types.

Unknown JSON fields and SSE events are retained. Unknown API paths are rejected in strict mode. A future compatibility mode may pass them through only after explicit user activation and must mark the traffic as uninspected.

## 7. Request and response flow

```text
receive request
  -> enforce source, method, and size limits
  -> authenticate local key
  -> resolve connection and protocol
  -> parse a supported API structure
  -> extract approved text fields
  -> detect and deconflict sensitive spans
  -> apply allow/monitor/block/mask/pseudonymize policy
  -> replace local authentication with upstream authentication
  -> forward without cross-origin credential redirects
  -> parse normal JSON or SSE response
  -> restore exact pseudonym tokens in supported output fields
  -> record metadata-only activity
  -> return to the client
```

### 7.1 Streaming

The gateway parses SSE frames rather than replacing arbitrary TCP chunks. Text deltas keep a bounded tail at least as long as the longest possible placeholder so a placeholder split across chunks can be restored.

Tool-call JSON deltas are accumulated per tool call until a valid value boundary is available. The adapter restores string values and re-emits compatible deltas. This can add tool-stream latency but does not require buffering an entire ordinary text response.

Unknown event types are forwarded unchanged. Thinking signatures and encrypted reasoning data are never transformed.

### 7.2 Errors

- Invalid local key: HTTP 401.
- Unsupported route: HTTP 404 using the profile protocol's error envelope.
- Oversized body: HTTP 413.
- Privacy policy block: a protocol-compatible error with code `privacy_policy_violation`.
- Malformed supported JSON or SSE: fail closed.
- Detector failure in strict mode: fail closed.
- Upstream network error: return a distinct proxy error; do not switch providers automatically.
- Upstream API error: preserve status and safe error structure, restoring only valid local placeholders in supported string fields.

Privacy errors include category names and a local request ID but never include original content or sensitive values.

OpenAI-profile errors use the OpenAI `{"error": {...}}` envelope. Anthropic-profile errors use the Anthropic `{"type": "error", "error": {...}}` envelope. Local error codes remain under the product-owned `privacy_*` or `gateway_*` namespaces so clients and users can distinguish them from upstream errors.

## 8. Privacy engine

### 8.1 Entity groups

#### High-risk credentials

Default action: block.

- known provider API keys;
- cloud access credentials;
- bearer tokens and JWTs;
- PEM, SSH, and PGP private keys;
- database and message-broker connection strings;
- URLs containing credentials;
- password and secret assignments with sufficient contextual evidence;
- user-defined credential patterns.

#### Validatable identity and network identifiers

Default action: pseudonymize or mask.

- Chinese resident identity numbers with checksum validation;
- Unified Social Credit Codes with checksum validation;
- bank card numbers with Luhn validation;
- mainland Chinese phone numbers;
- email addresses;
- IPv4, IPv6, and MAC addresses;
- internal hostnames and domains;
- coordinates;
- configured account-number formats.

#### User-defined sensitive entities

Default action: pseudonymize.

- people;
- organizations and departments;
- project and customer names;
- supplier and product codenames;
- equipment identifiers;
- internal terminology;
- user-defined regular expressions.

The MVP does not use an unvalidated model to claim complete Chinese name, organization, or address recognition.

### 8.2 Detection pipeline

1. Build a normalized matching view while retaining a mapping to original character positions.
2. Run credential patterns, checksum detectors, network detectors, dictionary matching, and user regexes.
3. Convert detections to a common span type.
4. Resolve overlap by detector priority, confidence, and longest span.
5. Evaluate policy.
6. Replace from the end of the original text toward the beginning.

Priority is:

```text
private key and credential
> checksum-validated identifier
> user exact term
> network identifier
> generic pattern
```

### 8.3 Actions

- `allow`: do not modify and do not report as a violation.
- `monitor`: forward unchanged and record category/count only.
- `block`: do not contact the upstream.
- `mask`: irreversibly replace the span.
- `pseudonymize`: replace with a stable token and store an encrypted reverse mapping.

The synchronous API path has no interactive approval prompt. A blocked request fails immediately.

### 8.4 Placeholder format and mapping

Example placeholders:

```text
__LPG_PERSON_K7M4Q2...__
__LPG_PROJECT_R9H2A6...__
__LPG_IPV4_F3D8N5...__
```

The token contains at least 80 bits of an HMAC-derived identifier. It never uses an incrementing counter.

```text
placeholder_id = HMAC-SHA-256(
  profile_master_key,
  entity_type || normalized_original
)
```

The same normalized value within one connection profile receives the same placeholder. Different profiles produce different placeholders. This supports client-resubmitted histories and server-side conversation references, provided the profile mapping still exists.

Reverse mappings are encrypted with AES-256-GCM. Master material is held by the Windows credential store. Deleting a profile deletes its mappings. Users can explicitly clear mappings. Existing placeholder-shaped input is escaped or rejected to prevent restoration confusion.

## 9. Policy model

Policy precedence:

```text
non-disableable security invariants
  -> built-in policy template
  -> connection override
  -> explicit whitelist
```

Built-in templates:

- Strict: block credentials, pseudonymize all recognized PII, reject unknown protocol structures.
- Balanced: block credentials, pseudonymize strong identifiers, monitor ordinary network identifiers.
- Monitor: block only private keys and explicit passwords; report other supported findings without modifying them.

The first-run default is Strict. Before enabling a profile, the setup wizard runs a local compatibility and policy preview.

## 10. Desktop experience

### 10.1 First-run wizard

```text
welcome
  -> create upstream connection
  -> save upstream credential
  -> choose privacy policy
  -> run local gateway test
  -> copy environment variables
```

### 10.2 Dashboard

Display:

- gateway status and port;
- request and block counts;
- processed entity-category counts;
- connection health;
- recent metadata-only errors.

Do not display or store prompt and response content.

### 10.3 Connection management

Each profile contains:

- display name;
- protocol type;
- upstream base URL;
- credential-store reference;
- generated local key;
- selected policy;
- connection-test action;
- copyable client configuration;
- local-key rotation and revocation.

### 10.4 Policy center

Users configure category actions, exact sensitive terms, custom regexes, and whitelists. Rule export excludes credentials and pseudonym mappings.

### 10.5 Local test console

The console shows:

```text
original input
  -> detections
  -> exact outbound transformed text
  -> restoration preview
```

It performs local processing by default. An upstream test requires an explicit action and displays the transformed outbound content before transmission.

### 10.6 Activity records

Persist:

- timestamp;
- profile ID;
- API family and endpoint;
- model string;
- status code;
- latency;
- detection categories and counts;
- policy actions;
- local request ID.

Never persist:

- prompt or response bodies;
- original sensitive values;
- pseudonym maps in the activity database;
- upstream or local keys;
- full request headers.

## 11. Security boundaries and threat model

### 11.1 Protected assets

- upstream provider credentials;
- sensitive request spans;
- reversible pseudonym mappings;
- custom sensitive dictionaries;
- activity metadata.

### 11.2 In-scope threats

- accidental transmission of supported secrets and identifiers;
- unauthorized local requests without a local capability key;
- malicious or mistaken custom upstream URLs;
- credential leakage through redirects;
- secret leakage through logs, crash reports, SQLite, or UI IPC;
- malformed JSON/SSE that bypasses inspection;
- placeholder injection and mapping confusion;
- oversized or slow requests that exhaust the local gateway.

### 11.3 Out-of-scope threats

- a fully compromised Windows account or kernel;
- malware that can read process memory or the user's credential store;
- traffic that does not pass through the gateway;
- information inferred from non-PII context that the configured detectors do not recognize;
- the upstream model intentionally rewriting placeholder syntax so it cannot be restored;
- provider retention or subpoena of content that the user explicitly allowed through.

## 12. Testing strategy

### 12.1 Unit tests

Cover every detector's positive, negative, and boundary cases; Chinese checksums; Unicode position mapping; overlap resolution; policy precedence; placeholder stability; profile isolation; authenticated encryption; fake-placeholder input; protocol field selection; and preservation of unknown fields.

### 12.2 Property tests

Use generated data to verify:

- spans never overlap after resolution;
- transformations do not index outside the source;
- `restore(pseudonymize(x)) == x` for supported text;
- unrelated JSON fields retain semantic equality;
- every possible chunk boundary restores placeholders consistently;
- structured logs never contain generated canary secrets.

### 12.3 Protocol fixtures

Maintain official-document-derived and synthetic fixtures for OpenAI Chat Completions, OpenAI Responses, Anthropic Messages, tool calls, stream events, and errors. Never use real user prompts.

### 12.4 SDK compatibility

On Windows CI, start a mock upstream and the local gateway, then test pinned versions of:

- OpenAI Python SDK;
- OpenAI JavaScript SDK;
- Anthropic Python SDK;
- Anthropic TypeScript SDK;
- curl.

Release notes list exactly which versions pass. Compatibility claims do not extend to untested past or future versions.

### 12.5 Security tests

Test loopback binding, local-key revocation, disabled CORS, body limits, slow requests, redirect credential stripping, malicious URLs, header injection, malformed JSON and SSE, unknown events, upstream interruption, database/log/crash-output leakage, and WebView command authorization.

### 12.6 Privacy evaluation corpus

Create a synthetic, versioned corpus:

```text
privacy-evals/
├── credentials.jsonl
├── china-identifiers.jsonl
├── network-identifiers.jsonl
├── custom-terms.jsonl
├── benign-lookalikes.jsonl
└── mixed-protocol-payloads.jsonl
```

No real credentials or personal information may enter the corpus.

### 12.7 MVP targets

| Metric | Target |
|---|---:|
| Recall on explicitly supported credential fixtures | 100% |
| Recall on valid checksum-backed Chinese identifier fixtures | 100% |
| False identification of checksum-invalid ID/card fixtures | 0% |
| Exact custom-dictionary recall | 100% |
| Protocol golden-fixture pass rate | 100% |
| Pinned official SDK matrix pass rate | 100% |
| Unredacted canary secrets reaching the mock upstream | 0 |
| Added P95 scan latency for 100 KB text | <= 20 ms, excluding upstream |
| Added P95 first-event latency for ordinary text SSE | <= 30 ms |
| Idle gateway-core memory excluding WebView | <= 60 MB |
| Prompt or real key values in logs/activity DB | 0 |

These are versioned fixture targets, not claims of perfect real-world detection.

## 13. Reference-source policy

After written-spec approval, shallow-clone and pin:

1. `https://github.com/jayluxferro/llm-redactor` — MIT; direct proxy, detector, restoration, and leak-evaluation reference.
2. `https://github.com/openziti/llm-gateway` — Apache-2.0; single-binary routing, SSE, virtual-key, and Windows-release reference.
3. `https://github.com/data-privacy-stack/presidio` — MIT; recognizer interfaces, overlap handling, and anonymization reference.
4. `https://github.com/sherwinvishesh/SurrogateShield` — Apache-2.0; encrypted map and utility-evaluation reference.
5. `https://github.com/openai/privacy-filter` — Apache-2.0; future local-model and evaluation-interface reference.

Do not clone LiteLLM or Portkey for the MVP because their scope is substantially larger than this product. Do not clone or copy `WangYihang/LLM-Redactor` unless it gains an explicit license; its current public page does not show one.

Place reference clones under ignored `references/`. Commit `docs/research/upstreams.md` with URL, retrieval date, commit SHA, license, license-file hash, research purpose, accepted ideas, rejected ideas, and any copied-code attribution. Do not use Git submodules and do not ship reference runtimes.

The implementation is independent by default. Any copied MIT/Apache source must retain required notices and be recorded at file level.

## 14. Delivery stages

### Stage 0: research baseline

- initialize project governance and security documents;
- clone and pin the approved references;
- write a source audit and threat model;
- establish protocol fixtures.

### Stage 1: privacy core

- create the Rust workspace and privacy types;
- implement deterministic detectors and user dictionaries;
- implement overlap resolution and policies;
- implement HMAC placeholders and encrypted mappings;
- add unit, property, and privacy-evaluation tests;
- expose a development CLI for local detection and restoration.

### Stage 2: OpenAI gateway

- implement loopback listener and local keys;
- implement profile persistence and credential storage;
- implement Chat Completions and Responses adapters;
- implement normal JSON and SSE transport;
- add mock-upstream and pinned-SDK tests.

### Stage 3: Anthropic gateway

- implement Messages and token counting;
- implement tool input and tool-result handling;
- implement Anthropic SSE and unknown-event behavior;
- add pinned-SDK tests.

### Stage 4: desktop application

- create the first-run wizard;
- create connection and policy management;
- create the local test console and activity view;
- add system tray and autostart;
- package an unsigned internal NSIS build.

### Stage 5: hardening and release candidate

- add resource limits and fault injection;
- scan logs, crash outputs, and persistence for canary leakage;
- audit dependencies and licenses;
- test install, upgrade, uninstall, tray lifecycle, and credential cleanup;
- add signed update and release procedures;
- publish exact compatibility and limitation documentation.

## 15. MVP completion criteria

The MVP is complete only when:

1. A new user can create a profile and copy working client configuration within five minutes.
2. The pinned OpenAI and Anthropic SDK matrix passes on Windows.
3. No supported unredacted canary secret reaches the mock upstream.
4. Profiles, credentials, and mappings survive restart and are deleted when the profile is deleted.
5. Default logs and activity storage contain no request or response body.
6. Unknown protocol paths and malformed supported payloads do not pass silently in strict mode.
7. Install, tray, exit, upgrade, and uninstall behavior is reproducible.
8. UI statements and documentation describe the same coverage boundaries.

## 16. Primary references

- Tauri overview: https://v2.tauri.app/start/
- Tauri Stronghold: https://v2.tauri.app/plugin/stronghold/
- Windows App SDK: https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/
- Electron security guidance: https://www.electronjs.org/docs/latest/tutorial/security
- OpenAI Chat Completions API: https://platform.openai.com/docs/api-reference/chat
- OpenAI Responses streaming events: https://platform.openai.com/docs/api-reference/responses-streaming
- Anthropic streaming Messages: https://platform.claude.com/docs/en/build-with-claude/streaming
- LLM-Redactor: https://github.com/jayluxferro/llm-redactor
- OpenZiti LLM Gateway: https://github.com/openziti/llm-gateway
- Presidio: https://github.com/data-privacy-stack/presidio
- SurrogateShield: https://github.com/sherwinvishesh/SurrogateShield
- OpenAI Privacy Filter: https://github.com/openai/privacy-filter
