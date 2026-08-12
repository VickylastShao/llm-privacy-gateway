# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases use
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Windows-first local OpenAI-compatible and Anthropic Messages privacy gateway.
- Deterministic sensitive-data detection, blocking, masking, and reversible pseudonymization.
- Local privacy activity, custom dictionaries, provider presets, environment discovery, and
  provider-reported Token usage summaries.
- Verified regular-key quota or balance queries for DeepSeek, Kimi, SiliconFlow, and OpenRouter.
- Connection editing and authenticated upstream health/latency checks.
- Explicit recovery for saved connections with missing or invalid local security data.
- Integrity-checked, atomic migration of the preview-identity SQLite database without overwriting
  an existing destination or deleting the legacy database.
- Transactional activity-database schema migration with bounded SQLite lock waiting.
- Protocol-aware restoration for nested OpenAI and Anthropic tool values and final streaming events.
- Keyboard-complete overflow actions, native modal confirmation behavior, and visible clipboard
  failure feedback in the single-page desktop workflow.
- Coverage-aware request filtering: strict profiles block uninspected content blocks, while balanced
  and monitor profiles forward them with bounded, payload-free activity feedback.
- A shared provider catalog used by both the desktop UI and environment discovery.
- Distinct account-balance and API-key-limit labels for provider quota responses.

### Security

- Upstream credentials use the operating-system credential vault.
- The gateway listens only on loopback, rejects unsafe upstream URLs, disables redirects, and
  does not persist request or response bodies.
- Environment-discovered credentials remain outside the WebView, expire after five minutes, and
  are cleared by a backend timer, after import, or after cancellation. Discovery starts
  automatically when the add-connection form opens; importing a discovered credential remains an
  explicit action.
- Editing a connection cannot reuse a saved upstream key after the network origin changes; a new
  key is required before the connection can target another server.
- GitHub CodeQL analysis and build-provenance attestations for public release artifacts.

## [0.1.0] - Unreleased

Initial public preview. This version will be dated only after the signed Windows installer and
release artifacts pass the release checklist.

[Unreleased]: https://github.com/VickylastShao/llm-privacy-gateway/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/VickylastShao/llm-privacy-gateway/releases/tag/v0.1.0
