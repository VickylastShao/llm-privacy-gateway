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

It does not protect traffic that bypasses the local gateway. It does not claim complete detection or anonymization. Realtime, image/audio/video, Batch, webhook, cloud-provider-specific protocols, and browser-only chat sites are outside the current supported boundary.

## Development

Use the Rust toolchain pinned in `rust-toolchain.toml`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo run -p lpg-cli -- eval privacy-evals
```

The desktop frontend requires Node 24 LTS and npm. Build the Windows desktop package with `npm run desktop:build`; the release build uses the Windows GUI subsystem and does not open a console window.
