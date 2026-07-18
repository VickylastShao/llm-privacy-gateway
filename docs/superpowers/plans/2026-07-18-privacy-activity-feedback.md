# Privacy Activity Feedback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a single-page privacy activity panel that shows safe real-time detection feedback, persists metadata for seven days, and never persists sensitive matches.

**Architecture:** Privacy transformations append safe decisions to a request report. The gateway publishes one safe event per request through an abstract sink; the desktop activity service keeps previews in bounded process memory and writes metadata asynchronously to SQLite. React polls a Tauri query command and renders the newest activity below model connections.

**Tech Stack:** Rust 2024, Axum, Tokio, rusqlite, Tauri 2, React 19, TypeScript, Vitest, Testing Library

---

## File structure

```text
crates/privacy-core/src/report.rs                 Safe entity labels, actions, masking, request report
crates/privacy-core/src/engine.rs                 Report-aware transformation
crates/privacy-core/tests/report.rs               Masking and blocked-report tests
crates/protocol-openai/src/{chat,responses}.rs    Request-report aggregation
crates/protocol-anthropic/src/messages.rs         Request-report aggregation without signed-block mutation
crates/gateway/src/activity.rs                    ActivityEvent and ActivitySink contract
crates/gateway/src/{runtime,server}.rs             Sink injection and final request event publication
crates/activity-store/src/lib.rs                  Schema v2, retention, newest-first queries
apps/desktop/src-tauri/src/activity.rs             Session previews and background persistence
apps/desktop/src-tauri/src/lib.rs                  Service setup and query_activity command
apps/desktop/src/model.ts                          Activity API types
apps/desktop/src/desktop-api.ts                    Activity query bridge
apps/desktop/src/components/PrivacyActivity.tsx    Single-page activity presentation
apps/desktop/src/{App,i18n,styles}.ts(x|css)        Polling, localization, and visual integration
```

### Task 1: Safe privacy reports

**Files:**
- Create: `crates/privacy-core/src/report.rs`
- Create: `crates/privacy-core/tests/report.rs`
- Modify: `crates/privacy-core/Cargo.toml`
- Modify: `crates/privacy-core/src/lib.rs`
- Modify: `crates/privacy-core/src/engine.rs`

- [ ] **Step 1: Write failing tests**

Add tests that call `transform_reported` with deterministic detections and assert:

```rust
let mut report = RequestPrivacyReport::default();
let result = engine.transform_reported(text, &Policy::strict(), &mut map, &mut report);
assert!(matches!(result, Err(PrivacyError::Blocked)));
assert_eq!(report.category_counts().get("api_key"), Some(&1));
assert!(!serde_json::to_string(&report)?.contains(secret));
```

Cover API-key, national-ID, bank-card, IPv4, IPv6/MAC, and no-preview credential rules.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p privacy-core --test report --locked`

Expected: compilation fails because `RequestPrivacyReport` and `transform_reported` do not exist.

- [ ] **Step 3: Implement the safe report**

Move `serde` into the crate's normal dependencies and define stable serializable values:

```rust
pub struct PrivacyFinding { kind: String, action: String, preview: Option<String> }
#[derive(Default)]
pub struct RequestPrivacyReport { findings: Vec<PrivacyFinding> }
```

Implement `record(&Decision, value)` with type-specific masking from the approved specification. Implement aggregation methods for category counts, distinct actions, and at most three deduplicated previews per kind. Add `transform_reported`; append decisions before returning `PrivacyError::Blocked`. Keep `transform` as a wrapper with a discarded report.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p privacy-core --test report --locked && cargo test -p privacy-core --locked`

Expected: all privacy-core tests pass and serialized reports contain no canary value.

- [ ] **Step 5: Commit**

```bash
git add crates/privacy-core
git commit -m "feat: report safe privacy decisions"
```

### Task 2: Aggregate protocol reports

**Files:**
- Modify: `crates/protocol-openai/src/chat.rs`
- Modify: `crates/protocol-openai/src/responses.rs`
- Modify: `crates/protocol-openai/src/lib.rs`
- Modify: `crates/protocol-openai/tests/requests.rs`
- Modify: `crates/protocol-anthropic/src/messages.rs`
- Modify: `crates/protocol-anthropic/tests/messages.rs`

- [ ] **Step 1: Write failing adapter tests**

Call new report-aware entry points with multiple sensitive fields and assert one mutable report receives every occurrence, including findings completed before a blocked field. Add an Anthropic raw-message test that asserts signed thinking bytes are unchanged.

- [ ] **Step 2: Verify RED**

Run:

```bash
cargo test -p protocol-openai --test requests --locked
cargo test -p protocol-anthropic --test messages --locked
```

Expected: new report-aware functions are missing.

- [ ] **Step 3: Implement report-aware adapters**

Add `_with_report` variants that accept `&mut RequestPrivacyReport`; pass the same report through every approved string field. Keep existing public functions as compatibility wrappers using a temporary report. Do not broaden field allowlists or parse signed thinking blocks.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p protocol-openai --locked && cargo test -p protocol-anthropic --locked`

Expected: new aggregation tests and all byte-preservation tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/protocol-openai crates/protocol-anthropic
git commit -m "feat: aggregate protocol privacy reports"
```

### Task 3: Publish one gateway activity per detected request

**Files:**
- Create: `crates/gateway/src/activity.rs`
- Modify: `crates/gateway/src/lib.rs`
- Modify: `crates/gateway/src/runtime.rs`
- Modify: `crates/gateway/src/server.rs`
- Modify: `crates/gateway/tests/security_shell.rs`

- [ ] **Step 1: Write failing gateway tests**

Create a collecting sink and assert:

```rust
assert_eq!(sink.events().len(), 1);
assert_eq!(sink.events()[0].status, 403);
assert_eq!(sink.events()[0].category_counts["api_key"], 1);
assert!(!format!("{:?}", sink.events()[0]).contains(CANARY));
```

Cover blocked, transformed, monitored, clean, and rejecting-sink requests.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p gateway --test security_shell --locked`

Expected: activity sink APIs are missing.

- [ ] **Step 3: Implement the gateway contract**

Define `ActivityEvent`, `ActivitySink`, and `NoopActivitySink`. Add an injectable sink to `GatewayState`, preserving `GatewayState::new` as the no-op constructor. At request start, capture timestamp, request ID, endpoint, top-level model, and timer. Pass one request report through transformation. Publish one event after blocked/local failure or after upstream headers are received, only when the report is nonempty. Ignore sink rejection when forming the HTTP response.

- [ ] **Step 4: Verify GREEN and performance**

Run:

```bash
cargo test -p gateway --locked
cargo test -p gateway --test performance_budget --release --locked
```

Expected: gateway/security tests pass and existing release budgets remain within their gates.

- [ ] **Step 5: Commit**

```bash
git add crates/gateway
git commit -m "feat: publish gateway privacy activity"
```

### Task 4: Persist seven-day metadata safely

**Files:**
- Modify: `crates/activity-store/src/lib.rs`
- Modify: `crates/activity-store/tests/repository.rs`
- Modify: `crates/activity-store/tests/canary_leak.rs`

- [ ] **Step 1: Write failing repository tests**

Construct a version-1 database, open it, record timestamped activities, and assert newest-first limit, 168-hour pruning, 10,000-row cap, and absence of canary text from the SQLite bytes.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p activity-store --locked`

Expected: timestamped activity query and retention APIs are missing.

- [ ] **Step 3: Implement schema version 2 and repository APIs**

Add `id` and `occurred_at_ms` to the Rust activity model and migrate the table with an indexed timestamp. Implement `record_activity`, `activities(limit)`, `activity_count`, and `prune_activities(now_ms)`. Cap query limit at 100, remove rows older than `now_ms - 604_800_000`, then retain the newest 10,000 rows.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p activity-store --locked`

Expected: migration, retention, ordering, and canary tests pass.

- [ ] **Step 5: Commit**

```bash
git add crates/activity-store
git commit -m "feat: retain privacy activity metadata"
```

### Task 5: Add the desktop activity service and Tauri query

**Files:**
- Create: `apps/desktop/src-tauri/src/activity.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

- [ ] **Step 1: Write failing service tests**

Test an in-memory or temporary repository through `ActivityService`: accepted events are immediately queryable with previews, persisted metadata survives service recreation without previews, pruning runs, and a full/closed persistence channel increments `persist_dropped_count`.

- [ ] **Step 2: Verify RED on Windows-independent Rust code**

Run: `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml activity --locked`

Expected on a native build host: activity module and query response types are missing. On WSL, use the cross-platform crate-level service tests if Tauri's GTK build dependencies are unavailable.

- [ ] **Step 3: Implement the service**

Use a bounded 1,024-event channel and a dedicated writer thread. Keep up to 10,000 safe session views in `VecDeque`; enqueue only persistent metadata. Merge session and repository rows by activity ID. Add the service to `DesktopState`, inject it into `GatewayState`, replace the empty `query_activity` command with `query_activity(limit)`, and run retention cleanup at open/query.

- [ ] **Step 4: Verify GREEN**

Run the desktop Rust tests on Windows and `cargo test --workspace --locked` for shared crates.

Expected: service lifecycle and existing desktop profile tests pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src-tauri
git commit -m "feat: expose desktop privacy activity"
```

### Task 6: Render privacy activity in the single page

**Files:**
- Create: `apps/desktop/src/components/PrivacyActivity.tsx`
- Modify: `apps/desktop/src/model.ts`
- Modify: `apps/desktop/src/desktop-api.ts`
- Modify: `apps/desktop/src/App.tsx`
- Modify: `apps/desktop/src/i18n.ts`
- Modify: `apps/desktop/src/styles.css`
- Modify: `apps/desktop/src/main.test.tsx`
- Modify: `apps/desktop/src/styles.test.ts`

- [ ] **Step 1: Write failing UI tests**

Extend the fake desktop API with `queryActivity`. Use fake timers to assert empty state, one-second polling while running, final refresh after stop, bilingual labels, row details, preview-expired text, persistence-loss warning, and expansion from 10 to 100.

- [ ] **Step 2: Verify RED**

Run: `npx vitest run apps/desktop/src/main.test.tsx apps/desktop/src/styles.test.ts`

Expected: activity API, component, and translations are missing.

- [ ] **Step 3: Implement typed activity UI**

Add `PrivacyActivityResponse`, `PrivacyActivityItem`, and preview types to `model.ts`. Add `queryActivity(limit)` to `DesktopApi`. Render the activity section below connections when a profile exists. Poll without stealing focus; preserve expanded IDs across refresh; show 10 rows initially and query 100 after `Show more`. Add all strings to both typed catalogs and add responsive activity styles.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
npm test
npm run lint
npm run build
```

Expected: all frontend, SDK, type, and production build checks pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src
git commit -m "feat: show privacy activity feedback"
```

### Task 7: Full security and Windows release verification

**Files:**
- Modify: `docs/release-checklist.md`

- [ ] **Step 1: Document privacy-activity release checks**

Add checks for blocked/transformed feedback, seven-day metadata, session preview destruction, persistence-loss warning, and SQLite canary absence.

- [ ] **Step 2: Run the complete Linux/shared verification**

Run:

```bash
npm test
npm run lint
npm run build
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
```

Expected: all commands pass; `cargo deny` may report existing duplicate-version warnings but advisories, bans, licenses, and sources remain `ok`.

- [ ] **Step 3: Build and inspect the Windows release**

From a native Windows checkout with VS 2022 and Windows SDK environment loaded, run:

```text
npm ci
npm run build
npm run desktop:build -- --bundles nsis
```

Expected: release executable and NSIS installer are produced. Launch the executable, create a synthetic blocked request, confirm the activity row appears, and confirm `dumpbin /headers` reports subsystem `Windows GUI`.

- [ ] **Step 4: Commit documentation**

```bash
git add docs/release-checklist.md
git commit -m "docs: verify privacy activity release"
```
