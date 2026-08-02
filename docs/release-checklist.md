# Release checklist

- Run Rust format, clippy, tests, and dependency audit.
- Run frontend install, tests, lint, and Windows Tauri build on Node 24 LTS.
- Run synthetic canary leak checks through every supported route.
- Confirm the CI P95 gates: 100KB privacy scan at or below 20ms and ordinary SSE first-event restoration at or below 30ms.
- Verify loopback binding, upgrade persistence, key rotation, profile deletion, and credential-removal behavior on clean Windows.
- Install the generated NSIS package and launch the Start Menu shortcut. Confirm the main window opens without a console or `conhost.exe` window.
- Repeat a debug launch and confirm console diagnostics remain available to developers.
- Send synthetic blocked and transformed requests and confirm their privacy activity appears without raw canary values.
- Restart the desktop app and confirm seven-day activity metadata remains while session-only previews are gone.
- Create a strict profile with a custom sensitive term and a mobile number, restart the app, then confirm both are pseudonymized and restored through non-streaming and SSE requests.
- Restart between request and response-replay checks and confirm encrypted pseudonym mappings still restore without plaintext appearing in SQLite.
- Confirm persistence failures produce a visible activity warning without changing gateway responses.
- Inspect the SQLite database and confirm request bodies, response bodies, original matches, and preview values are absent.
- Publish exact SDK versions and supported route limitations.
