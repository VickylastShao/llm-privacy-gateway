# Local LLM Privacy Gateway MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Build a Windows-first Tauri desktop application that exposes loopback OpenAI and Anthropic endpoints and locally blocks, masks, or reversibly pseudonymizes supported sensitive text.

**Architecture:** A Rust workspace separates detection, encrypted mappings, protocol adapters, persistence, and the Axum gateway. A Tauri 2 + React control plane configures profiles and policies but cannot retrieve provider secrets. Protocol adapters select mutable fields; the privacy engine never recursively edits unknown JSON.

**Tech Stack:** Rust 1.97.0, Tokio, Axum, Reqwest/Rustls, Serde/RawValue, SQLite/Rusqlite, Windows Credential Manager, RustCrypto, Tauri 2, React, TypeScript, Vite, Node.js 24 LTS, Vitest, Playwright, GitHub Actions.

---

## File map

~~~text
Cargo.toml                         Rust workspace
crates/privacy-core/              Detectors, policies, transformation
crates/session-map/               HMAC tokens, AES-GCM mappings
crates/protocol-openai/           OpenAI JSON and SSE
crates/protocol-anthropic/        Anthropic JSON and SSE
crates/secret-store/              Windows Credential Manager
crates/activity-store/            SQLite metadata persistence
crates/gateway/                   Listener, authentication, forwarding
apps/cli/                         Development CLI and eval runner
apps/desktop/                     React and Tauri application
fixtures/                         Protocol fixtures
privacy-evals/                    Synthetic privacy fixtures
tests/                            SDK compatibility and leak tests
~~~

## Task 1: Bootstrap the workspace

**Files:**
- Create: rust-toolchain.toml
- Create: Cargo.toml
- Create: deny.toml
- Create: apps/cli/Cargo.toml
- Create: apps/cli/src/main.rs
- Create: each crate Cargo.toml and src/lib.rs listed in the file map

- [ ] **Step 1: Install and verify tools**

~~~bash
rustup toolchain install 1.97.0 --profile minimal --component clippy,rustfmt
rustc +1.97.0 --version
~~~

Expected: rustc 1.97.0. Use Node.js 24.17.0 LTS for desktop tasks.

- [ ] **Step 2: Create the workspace**

Set resolver 3, edition 2024, rust-version 1.97, Apache-2.0, and forbid unsafe code. Add workspace dependencies: aes-gcm 0.10, aho-corasick 1, axum 0.8, base32 0.5, hmac 0.12, rand 0.9, regex 1, reqwest 0.12 with rustls/json/stream, rusqlite 0.37 bundled, secrecy 0.10, serde 1, serde_json 1 with raw_value, sha2 0.10, subtle 2, thiserror 2, tokio 1, tower 0.5, tower-http 0.6, tracing 0.1, url 2, and uuid 1.

Each initial library exports:

~~~rust
#![forbid(unsafe_code)]
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
~~~

- [ ] **Step 3: Add dependency policy**

deny.toml permits Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-3.0, and Zlib; denies yanked crates, wildcards, unknown registries, and unknown Git sources.

- [ ] **Step 4: Verify and commit**

~~~bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add Cargo.toml rust-toolchain.toml deny.toml apps crates
git commit -m "build: bootstrap Rust privacy gateway workspace"
~~~

Expected: all checks pass.

## Task 2: Define privacy types and detector contracts

**Files:**
- Create: crates/privacy-core/src/types.rs
- Create: crates/privacy-core/src/detector.rs
- Create: crates/privacy-core/tests/types.rs
- Modify: crates/privacy-core/src/lib.rs

- [ ] **Step 1: Write failing tests**

~~~rust
#[test]
fn rejects_invalid_span() {
    let result = Detection::new(EntityKind::Email, 8, 3, 100, "email_regex");
    assert!(result.is_err());
}
~~~

Run: cargo test -p privacy-core --test types. Expected: missing API compile failure.

- [ ] **Step 2: Implement exact contracts**

EntityKind contains ApiKey, PrivateKey, Password, ConnectionString, Jwt, ChinaResidentId, UnifiedSocialCreditCode, BankCard, Phone, Email, Ipv4, Ipv6, MacAddress, InternalHostname, Coordinate, and Custom(String).

~~~rust
pub trait Detector: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError>;
}
~~~

Detection stores kind, start, end, confidence u8, and source; it never stores matched plaintext.

- [ ] **Step 3: Verify and commit**

~~~bash
cargo test -p privacy-core
git add crates/privacy-core
git commit -m "feat: define typed privacy detections"
~~~

## Task 3: Implement deterministic detectors

**Files:**
- Create: crates/privacy-core/src/detectors/{mod,credentials,china_id,uscc,bank_card,network}.rs
- Create: crates/privacy-core/tests/detectors.rs
- Create: privacy-evals/{credentials,china-identifiers,benign-lookalikes}.jsonl

- [ ] **Step 1: Write table-driven tests**

Cover synthetic vendor keys, contextual generic secrets, JWT, PEM, valid/invalid Chinese ID, valid/invalid USCC, valid/invalid Luhn card, email, IPv4/IPv6, and internal hostnames.

~~~rust
#[test]
fn invalid_china_id_is_rejected() {
    let found = ChinaResidentIdDetector::default()
        .detect("110105194912310021").unwrap();
    assert!(found.is_empty());
}
~~~

- [ ] **Step 2: Implement validators**

Use ISO 7064 MOD 11-2 for Chinese IDs, MOD 31 for USCC, Luhn for cards, and std::net::IpAddr after candidate extraction. Compile regexes once. Generic secrets require assignment context such as api_key=, token:, password=, or secret:.

- [ ] **Step 3: Verify and commit**

~~~bash
cargo test -p privacy-core --test detectors
git add crates/privacy-core privacy-evals
git commit -m "feat: detect credentials and Chinese identifiers"
~~~

Expected: 100% recall on supported positive fixtures and zero matches on checksum-invalid fixtures.

## Task 4: Add dictionaries, overlap resolution, and policies

**Files:**
- Create: crates/privacy-core/src/{dictionary,overlap,policy,engine}.rs
- Create: crates/privacy-core/tests/engine.rs

- [ ] **Step 1: Write failing tests**

Test longest dictionary term, Unicode offsets, whitelist, strict/balanced/monitor templates, and credential priority over an overlapping custom term.

- [ ] **Step 2: Implement**

Use Aho-Corasick for exact user terms. Sort detections by start, security priority, confidence, and length; return no overlaps. Define PolicyAction as Allow, Monitor, Block, Mask, and Pseudonymize. Inspection contains decisions and counts but no copied plaintext.

- [ ] **Step 3: Verify and commit**

~~~bash
cargo test -p privacy-core
git add crates/privacy-core
git commit -m "feat: apply deterministic privacy policies"
~~~

## Task 5: Add encrypted reversible mappings

**Files:**
- Create: crates/session-map/src/{error,token,crypto,store}.rs
- Create: crates/session-map/tests/round_trip.rs
- Modify: crates/privacy-core/src/engine.rs

- [ ] **Step 1: Write failing tests**

Assert stable same-profile tokens, cross-profile separation, 80-bit minimum token IDs, ciphertext/plaintext inequality, AES-GCM tamper failure, deletion, and transform/restore round trip.

- [ ] **Step 2: Implement token generation**

~~~rust
pub fn token_for(key: &[u8; 32], kind: &str, value: &str) -> Result<String, MapError> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| MapError::InvalidKey)?;
    mac.update(kind.as_bytes());
    mac.update(&[0]);
    mac.update(value.as_bytes());
    let digest = mac.finalize().into_bytes();
    let id = base32::encode(base32::Alphabet::Rfc4648 { padding: false }, &digest[..10]);
    Ok(format!("__LPG_{}_{}__", kind.to_ascii_uppercase(), id))
}
~~~

- [ ] **Step 3: Implement encrypted records**

Encrypt each original with AES-256-GCM, random 96-bit nonce, and AAD profile_uuid + token + kind. Define a MappingBackend trait and in-memory test backend. Blocked inspections write nothing.

- [ ] **Step 4: Property-test and commit**

~~~bash
cargo test -p session-map -p privacy-core
git add crates/session-map crates/privacy-core
git commit -m "feat: add encrypted reversible pseudonyms"
~~~

## Task 6: Build the development CLI and eval runner

**Files:**
- Replace: apps/cli/src/main.rs
- Create: apps/cli/src/eval.rs
- Create: apps/cli/tests/cli.rs

- [ ] **Step 1: Test commands**

Test detect, transform, restore, and eval. JSON output includes kind/offset/source/confidence/action but not a second plaintext copy. A blocked request exits 2.

- [ ] **Step 2: Implement**

Use Clap. Read one argument or stdin. Require --show-sensitive-values to print originals. Evaluate JSONL exact spans and report precision, recall, and F1 per kind.

- [ ] **Step 3: Verify and commit**

~~~bash
cargo test -p lpg-cli
cargo run -p lpg-cli -- eval privacy-evals
git add apps/cli
git commit -m "feat: add local privacy evaluation CLI"
~~~

## Task 7: Persist profiles, secrets, mappings, and activity

**Files:**
- Create: crates/secret-store/src/{backend,windows,memory}.rs
- Create: crates/secret-store/tests/contract.rs
- Create: crates/activity-store/src/{schema,repository}.rs
- Create: crates/activity-store/tests/repository.rs
- Modify: crates/session-map/src/store.rs

- [ ] **Step 1: Test contracts**

SecretStore exposes put/get/delete. Windows accounts are upstream/<uuid>, local/<uuid>, and mapping/<uuid> under service llm-privacy-gateway. Non-Windows tests use memory storage.

- [ ] **Step 2: Implement SQLite**

Tables: profiles, policy_rules, pseudonym_records, activities, schema_migrations. Profiles contain protocol, URL, secret references, local-key digest, and policy. Activity contains endpoint, model, status, latency, category counts, actions, and request ID only.

- [ ] **Step 3: Enforce URLs**

Accept HTTPS. Accept HTTP only for localhost, 127.0.0.1, or ::1. Reject embedded credentials and fragments.

- [ ] **Step 4: Verify and commit**

~~~bash
cargo test -p secret-store -p activity-store -p session-map
git add crates/secret-store crates/activity-store crates/session-map
git commit -m "feat: persist profiles without plaintext secrets"
~~~

The test scans SQLite text/blob exports and fails if canary prompt or key values appear.

## Task 8: Implement the authenticated loopback gateway

**Files:**
- Create: crates/gateway/src/{config,error,auth,server,upstream}.rs
- Create: crates/gateway/tests/security_shell.rs
- Modify: apps/cli/src/main.rs

- [ ] **Step 1: Write gateway tests**

Test 127.0.0.1 binding, refusal of 0.0.0.0, missing/unknown keys, protocol mismatch, 4 MiB limit, absent CORS, concurrency 64, and request IDs.

- [ ] **Step 2: Implement authentication**

Accept OpenAI Bearer or Anthropic x-api-key. SHA-256 the presented key and compare with subtle::ConstantTimeEq. Resolve exactly one profile.

- [ ] **Step 3: Implement upstream safety**

Use one Reqwest client with Rustls, redirects disabled, 10-second connect timeout, and no total timeout on streams. Strip hop-by-hop and incoming auth headers; insert only the selected upstream credential.

- [ ] **Step 4: Verify and commit**

~~~bash
cargo test -p gateway
git add crates/gateway apps/cli
git commit -m "feat: add authenticated loopback gateway"
~~~

## Task 9: Implement OpenAI JSON adapters

**Files:**
- Create: crates/protocol-openai/src/{error,chat,responses,patch}.rs
- Create: crates/protocol-openai/tests/{chat,responses}.rs
- Create: fixtures/openai/chat/{basic,tools}.json
- Create: fixtures/openai/responses/basic.json
- Modify: crates/gateway/src/server.rs

- [ ] **Step 1: Write field-selection tests**

Include text, tools, IDs, schemas, Base64, images, encrypted reasoning, and unknown fields. Assert only approved natural-language and tool string values are mutable.

- [ ] **Step 2: Implement typed JSON paths**

A path is object keys and array indexes. Revalidate targets before patching. Reject duplicate and ancestor/descendant patch conflicts.

- [ ] **Step 3: Implement routes**

Chat selects message text, text content parts, tool-argument string values, and tool-role text. Responses selects instructions, string input, input_text, function arguments, and function outputs. IDs, schemas, media, file IDs, and encrypted reasoning stay opaque.

- [ ] **Step 4: Verify and commit**

~~~bash
cargo test -p protocol-openai -p gateway
git add crates/protocol-openai crates/gateway fixtures/openai
git commit -m "feat: filter OpenAI JSON requests"
~~~

## Task 10: Implement correct OpenAI SSE restoration

**Files:**
- Create: crates/protocol-openai/src/{sse,stream_restore}.rs
- Create: crates/protocol-openai/tests/streaming.rs
- Create: fixtures/openai/chat/stream.sse
- Create: fixtures/openai/responses/stream.sse
- Modify: crates/gateway/src/server.rs

- [ ] **Step 1: Add regression test**

Chunks "Contact __LPG_EMA" and "IL_K7M4Q2__ now" must emit exactly "Contact alice@example.com now", with no partial marker.

- [ ] **Step 2: Implement SSE framing**

Support event/data/comments, CRLF/LF, multiline data, blank termination, DONE, unknown events, and a 1 MiB event limit.

- [ ] **Step 3: Implement bounded restoration**

Hold only a suffix that can become __LPG_ or an unfinished token. Restore complete exact tokens; flush unmatched suffix at stream end. Buffer tool-argument deltas until their done event.

- [ ] **Step 4: Test every UTF-8 split and commit**

~~~bash
cargo test -p protocol-openai -p gateway streaming
git add crates/protocol-openai crates/gateway fixtures/openai
git commit -m "feat: restore OpenAI placeholders across SSE chunks"
~~~

## Task 11: Implement native Anthropic JSON and SSE

**Files:**
- Create: crates/protocol-anthropic/src/{error,messages,sse}.rs
- Create: crates/protocol-anthropic/tests/{messages,streaming}.rs
- Create: fixtures/anthropic/messages/{basic,signed-thinking}.json
- Create: fixtures/anthropic/messages/stream.sse
- Modify: crates/gateway/src/server.rs

- [ ] **Step 1: Test signed blocks**

A request has signed thinking blocks and a sensitive tool_result. After transformation, signed-block raw substrings are byte-identical while tool_result is pseudonymized.

- [ ] **Step 2: Implement RawValue preservation**

Store thinking, redacted_thinking, and unknown blocks as Box<RawValue>. Never rebuild, reorder, normalize, or scan them. Inspect system/message text and supported tool string values.

- [ ] **Step 3: Implement endpoints and streaming**

Apply identical transformation to messages and count_tokens. Preserve ping, error, unknown, thinking, and signature events. Restore text_delta. Buffer one input_json_delta block until content_block_stop, then emit valid transformed JSON.

- [ ] **Step 4: Verify and commit**

~~~bash
cargo test -p protocol-anthropic -p gateway
git add crates/protocol-anthropic crates/gateway fixtures/anthropic
git commit -m "feat: filter native Anthropic messages"
~~~

## Task 12: Build the Tauri and React control plane

**Files:**
- Create: package.json and package-lock.json
- Create: apps/desktop frontend/Vite/TypeScript files
- Create: apps/desktop/src/pages/{Setup,Dashboard,Connections,Policies,TestConsole,Activity}.tsx
- Create: apps/desktop/src-tauri/Cargo.toml
- Create: apps/desktop/src-tauri/src/{lib,commands}.rs
- Create: apps/desktop/src-tauri/{tauri.conf.json,capabilities/main.json}
- Modify: Cargo.toml

- [ ] **Step 1: Scaffold and test UI**

Pin Node 24.17.0. Test strict default, rejection of HTTP non-loopback URLs, one-time local configuration display, no secret in snapshots, and metadata-only activity columns.

- [ ] **Step 2: Implement narrow commands**

Commands cover profile/policy CRUD, local-key rotation, transform preview, gateway status/start/stop, activity query, and connection test. No command returns upstream keys, mapping keys, or full maps.

- [ ] **Step 3: Implement pages and tray**

Closing hides the window while gateway continues. Tray actions: Show, Start/Stop Gateway, Exit. Exit flushes metadata and stops the listener.

- [ ] **Step 4: Restrict capabilities**

Grant only window/event and registered commands. Deny unrestricted shell, filesystem, HTTP, clipboard, and process APIs. Use packaged assets and restrictive CSP.

- [ ] **Step 5: Verify and commit**

~~~bash
npm ci
npm test
npm run lint
cargo test --workspace
npm run desktop:build
git add package.json package-lock.json apps/desktop Cargo.toml
git commit -m "feat: add Windows privacy gateway desktop app"
~~~

## Task 13: Add SDK, leak, and performance gates

**Files:**
- Create: tests/compatibility/python/*
- Create: tests/compatibility/node/*
- Create: tests/security/canary-leak.ps1
- Create: crates/gateway/benches/scan.rs
- Create: .github/workflows/ci.yml

- [ ] **Step 1: Pin official SDK tests**

Pin OpenAI Python 2.45.0, Anthropic Python 0.116.0, OpenAI Node 6.46.0, and Anthropic TypeScript SDK 0.111.0. Test JSON, SSE, tools, errors, and token counting through a mock upstream. Print these resolved versions into CI artifacts and compatibility documentation.

- [ ] **Step 2: Add canary scanning**

Generate unique synthetic canaries, call every supported route, and scan mock captures, activity export, logs, and crash output. Fail on plaintext outside the encrypted mapping record.

- [ ] **Step 3: Add benchmarks**

Benchmark 1/10/100 KB mixed Chinese-English text, dictionaries of 10/1,000/10,000 terms, no-hit/multi-hit, and every placeholder split. Record Criterion output; release gate target is <=20 ms P95 scan overhead for 100 KB and <=30 ms P95 ordinary SSE first-event overhead on the pinned runner.

- [ ] **Step 4: Add CI and commit**

~~~bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test
git add tests crates/gateway/benches .github/workflows/ci.yml
git commit -m "test: gate SDK compatibility and privacy leakage"
~~~

CI also runs cargo-deny and builds Windows Tauri without exposing signing secrets.

## Task 14: Harden and package the release candidate

**Files:**
- Create: SECURITY.md
- Create: README.md
- Create: docs/security/threat-model.md
- Create: docs/{compatibility,privacy-boundaries,release-checklist}.md
- Create: .github/workflows/release.yml
- Modify: apps/desktop/src-tauri/tauri.conf.json

- [ ] **Step 1: Document boundaries**

State exact supported routes, pinned SDK matrix, fail-closed behavior, non-guarantee of complete anonymization, and inability to protect bypass traffic.

- [ ] **Step 2: Test lifecycle on clean Windows**

Test non-admin install, first run, tray, port collision, opt-in autostart, upgrade persistence, key rotation, profile deletion, uninstall, and explicit credential-removal choice.

- [ ] **Step 3: Configure protected signing**

Release workflow reads signing material only from a protected GitHub environment, signs binary and NSIS installer, produces SHA-256 checksums and SBOM, and creates a draft release. Pull requests never receive signing credentials.

- [ ] **Step 4: Run final gate and commit**

~~~bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
npm ci
npm test
npm run lint
npm run desktop:build
git add SECURITY.md README.md docs .github/workflows/release.yml apps/desktop/src-tauri/tauri.conf.json
git commit -m "chore: prepare privacy gateway release candidate"
~~~

Expected: all checks and Windows jobs pass; no unsupported compatibility claim appears.

## Final completion checklist

- [ ] Setup to working local profile takes no more than five minutes.
- [ ] Gateway listens only on loopback.
- [ ] Keys and prompts are absent from config, SQLite plaintext, logs, crash output, and UI state.
- [ ] Supported canaries never reach the mock upstream in plaintext.
- [ ] OpenAI Chat Completions and Responses pass JSON, SSE, tool, and error fixtures.
- [ ] Anthropic Messages and count_tokens pass JSON, SSE, tool, signed-thinking, and error fixtures.
- [ ] Signed Anthropic blocks are byte-identical.
- [ ] Placeholder restoration passes every UTF-8 split point.
- [ ] Unknown routes fail closed in Strict mode.
- [ ] Release notes list exact tested SDK versions and limitations.
