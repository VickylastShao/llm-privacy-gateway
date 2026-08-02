# Security policy

## Reporting a vulnerability

Report vulnerabilities through [GitHub private vulnerability reporting](https://github.com/VickylastShao/llm-privacy-gateway/security/advisories/new). Do not open a public issue before a fix is available.

Include the affected version, operating system, reproduction steps, and security impact. Do not include production API keys, prompts, documents, plaintext mappings, or other private data. Use synthetic values in reproductions.

## Supported versions

Before the first stable release, only the latest published preview receives security fixes. After a new preview is published, older previews are unsupported.

## Security invariants

The gateway must bind only to `127.0.0.1` or `::1`. Upstream credentials belong in the operating-system credential store; they must not be written to profiles, activity rows, logs, or crash reports. Mapping keys and plaintext mappings must remain local.
