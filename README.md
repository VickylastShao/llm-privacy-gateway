# LLM Privacy Gateway

Windows-first local gateway for API clients that use OpenAI-compatible or Anthropic Messages endpoints. It detects supported sensitive text before an upstream request and applies a per-connection strict, balanced, or monitor policy locally.

## Current MVP boundary

- Loopback-only gateway shell with local-key authentication.
- OpenAI Chat Completions/Responses JSON field selection and placeholder restoration primitives.
- Anthropic Messages JSON field selection that preserves thinking/signature blocks.
- Local encrypted mappings: AES-256-GCM records are persisted as ciphertext, nonce, token, and entity kind only.
- Deterministic detection for supported credentials, checksum-backed Chinese identifiers, bank cards, mainland Chinese mobile numbers, email/network identifiers, and exact custom sensitive terms.
- Per-connection custom terms stored in the operating-system credential vault rather than the ordinary profile database.
- Privacy activity containing bounded metadata and safe previews, without request or response bodies.
- Editable provider presets for OpenAI, Anthropic, DeepSeek, Gemini, OpenRouter, Groq, xAI, Mistral, Kimi, Alibaba Model Studio, and SiliconFlow, plus a custom Base URL mode.
- Automatic discovery of known API-key and Base-URL variable names when the add-connection form opens, including `ANTHROPIC_AUTH_TOKEN`. Import remains an explicit user choice. On Windows it checks the process, current-user, and system environments; raw keys never enter the WebView and are cleared by a five-minute backend timer, after import, or after cancellation.
- Authenticated connection testing with upstream HTTP status and latency, without storing the response body.
- Seven-day per-connection forwarded-request counts and Token totals collected from provider-reported response `usage` fields. Request and response bodies are not stored.
- On-demand quota or balance lookup for the verified regular-key endpoints offered by DeepSeek, Kimi, SiliconFlow, and OpenRouter.
- Visible startup recovery for saved connections whose local credentials, mapping key, or custom dictionary can no longer be loaded; removal requires explicit confirmation.
- Closing the window exits when protection is stopped. While protection is running it hides to the system tray, where **Show** reopens the window and **Exit** stops the gateway before quitting.

It does not protect traffic that bypasses the local gateway. It does not claim complete detection or anonymization. Realtime, image/audio/video inspection, file-content inspection, Batch, webhook, cloud-provider-specific protocols, and browser-only chat sites are outside the current supported boundary. Strict profiles block supported requests containing uninspected content blocks; balanced and monitor profiles forward them with a visible privacy-activity warning.

Current release evidence and external blockers are tracked in [`docs/release-status.md`](docs/release-status.md).
The local storage and network behavior is summarized in [`PRIVACY.md`](PRIVACY.md).
Clean-machine release validation is defined in
[`docs/clean-windows-validation.md`](docs/clean-windows-validation.md).
The protected Authenticode setup is documented in
[`docs/windows-code-signing.md`](docs/windows-code-signing.md).

The request count covers requests forwarded to an upstream provider. The separate metered-response count and Token totals cover only responses that contain a provider `usage` object. They are not billing records. OpenAI, Anthropic, Mistral, and other organization-level usage APIs require separate administrative credentials, so the desktop app does not attempt to query them with an ordinary inference key. Unsupported providers are labeled instead of being queried speculatively.

## Development

Use the Rust toolchain pinned in `rust-toolchain.toml`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lpg-cli -- eval privacy-evals
```

The Tauri Rust crate is intentionally outside the root Cargo workspace and must be checked
separately on Windows:

```powershell
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --locked
cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets --locked -- -D warnings
```

The desktop frontend requires Node 24 LTS and npm. Build the Windows desktop package with
`npm run desktop:build`; the release build uses the Windows GUI subsystem and does not open a
console window.

Before creating a public release tag, run `npm run check:release`. It fails closed while the
application identifier, publisher, repository links, release date, or private security-reporting
URL are incomplete.
