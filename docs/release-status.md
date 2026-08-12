# Release status

Last reviewed: 2026-08-13

## Verdict

**Blocked for public release.** The implemented MVP passes its code, test, dependency, and unsigned-build gates and now builds as a native Windows GUI application and unsigned NSIS installer. The final release preflight deliberately remains blocked while the changelog is undated. A public release still requires manual Windows interaction and upgrade checks, real-provider verification, and Authenticode signing material.

## Verified automatically

| Gate | Result |
| --- | --- |
| Rust workspace format, tests, and strict Clippy | Pass |
| Frontend type check, production build, and 38 Vitest tests | Pass |
| OpenAI and Anthropic Node SDK mock compatibility | Pass |
| OpenAI and Anthropic Python SDK mock compatibility | Pass |
| Rust dependency advisories, licenses, and sources | Pass with documented Tauri `rust-unic` unmaintained-advisory exceptions and non-blocking duplicate-dependency warnings |
| npm high/critical vulnerability audit | Pass on 2026-08-13 after upgrading transitive `nanoid` to 3.3.18 and `undici` to 7.29.0 |
| GitHub Dependabot alerts | Reviewed for the Windows-only release: `GHSA-wrw7-89jp-8q8g` affects the Linux/GTK-only `glib 0.18.5` dependency, which is absent from the `x86_64-pc-windows-msvc` graph; dismissed as not used and must be reopened before Linux support |
| Deterministic privacy evaluation corpus | Pass for the current synthetic corpus |
| Repository secret-pattern scan | Pass after classifying documented synthetic canaries |
| Automated full-history secret scan | Pass in GitHub CI with pinned Gitleaks Action 3.0.0; only the two documented synthetic detector-fixture paths are allowlisted for the JWT and private-key rules |
| Windows desktop Rust tests | Pass: 20 tests on the native MSVC target |
| Windows desktop strict Clippy | Pass on the native MSVC target |
| Preview-identity data migration | Pass: WAL-consistent backup, destination non-overwrite, integrity check, atomic publish, and source preservation |
| Activity schema migration | Pass: all schema changes are transactional; a forced migration failure preserves the legacy table and rows |
| Windows frontend/build runtime | Pass on Node 24.15.0 |
| Windows release executable | Pass: PE subsystem is `Windows GUI`; native launch remained alive without a console child process |
| Unsigned NSIS package | Pass: `LLM Privacy Gateway_0.1.0_x64-setup.exe` built and its SHA-256 value was independently verified |
| Current-host packaged-runner lifecycle | Pass with limitation: silent install/reinstall, launch, stopped-window exit, running-window tray hide, and data preservation worked, but Windows redirected the install into the Codex package's virtualized `LocalAppData`; this is not clean-user evidence |
| Local gateway lifecycle | Pass: temporary profile started the gateway, unauthenticated access returned 401, stop and profile cleanup succeeded, and no temporary plaintext canary remained in SQLite |
| Desktop interaction regression | Pass: clipboard failures are visible, menus support arrow/Home/End/Escape navigation, dialogs use native modal behavior, and startup no longer scrolls past recovery notices |
| Public-release preflight validation | Pass: unit tests cover ready and incomplete manifests; the current preflight intentionally fails because `0.1.0` has no final date |
| Unsupported content coverage | Pass locally: strict profiles block uninspected OpenAI/Anthropic content blocks; balanced and monitor profiles forward them with payload-free activity feedback |
| Provider catalog consistency | Pass locally: UI presets and environment discovery consume the same checked-in catalog |
| Static and provenance controls | Configured: Biome frontend lint, Rust/JavaScript CodeQL, and GitHub artifact attestations; GitHub runs remain required before merge/release |
| Permanent release identity | Pass locally: application identifier `com.vickylast.llmprivacygateway`, publisher `vickylast`, and repository links use `VickylastShao/llm-privacy-gateway` |
| GitHub release channel | Pass: the public `VickylastShao/llm-privacy-gateway` repository exists and private vulnerability reporting is enabled |
| Clean-runner lifecycle automation | Pass in [GitHub CI run 30747661226](https://github.com/VickylastShao/llm-privacy-gateway/actions/runs/30747661226): CI installed, launched, normally closed, reinstalled, uninstalled, and checked database preservation on a fresh `windows-latest` runner; the release workflow repeats it with signature verification enabled |

These results do not substitute for the Windows and real-provider checks below.

Latest local unsigned package:

- `LLM Privacy Gateway_0.1.0_x64-setup.exe`
- 4,378,272 bytes
- SHA-256: `f16a35794a1e80bf77f94605e43acb8043bb648811087ab05b15e44c78bb983c`
- PE subsystem: Windows GUI
- Authenticode status: `NotSigned` (expected for this local candidate)

The previously recorded controlled reinstall evidence applies to the earlier unsigned candidate; the current candidate still requires the clean-Windows and signed-release checks listed below.

## Release blockers

| Blocker | Required evidence or decision |
| --- | --- |
| Manual Windows interaction and upgrade | In a disposable Windows account or VM, verify start/stop, tray Show/Exit, and upgrade behavior; automated clean-runner installation lifecycle already passes |
| Code signing | Provide an Authenticode certificate and protected CI secrets, or select a managed signing service |
| Real-provider compatibility | Configure the two non-production API-key secrets and four Base URL/model variables documented in `docs/compatibility.md`; the release workflow now fails closed unless both provider checks pass before signing |
| Signed artifact verification | Verify the app and NSIS signatures, SHA-256 manifest, and SPDX SBOM generated by the release workflow |

## CC Switch feature boundary

CC Switch is a useful UX and operational reference, but its full feature set is not the target specification for this privacy product.

Included for the first public release:

- editable provider presets and custom Base URLs;
- automatic discovery of known environment configurations with explicit user-selected import;
- connection testing, provider-reported Token totals, and verified regular-key balance queries;
- local proxy operation, tray lifecycle, atomic profile/credential changes, and startup recovery;
- privacy detection feedback without storing request or response bodies.

Deferred until after the first release:

- multi-CLI configuration-file takeover and provider hot switching;
- automatic failover, circuit breaking, and format conversion beyond the two declared protocols;
- MCP, prompt, skill, session, and workspace management;
- cloud or WebDAV synchronization, deep links, and configuration sharing;
- automatic updating until a signed-update threat model and rollback path are implemented.

The deferred features expand scope and attack surface without improving the first release's core guarantee: sensitive data is processed locally before supported LLM API requests leave the device.
