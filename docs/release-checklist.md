# Release checklist

- Run Rust format, clippy, tests, and dependency audit.
- Run frontend install, tests, lint, and Windows Tauri build on Node 24 LTS.
- Run synthetic canary leak checks through every supported route.
- Confirm the CI P95 gates: 100KB privacy scan at or below 20ms and ordinary SSE first-event restoration at or below 30ms.
- Verify loopback binding, upgrade persistence, key rotation, profile deletion, and credential-removal behavior on clean Windows.
- Publish exact SDK versions and supported route limitations.
