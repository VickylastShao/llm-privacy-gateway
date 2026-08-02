# Threat model

Assets: upstream API keys, local API keys, mapping keys, plaintext prompts, original sensitive entities, and transformed responses.

Controls: loopback binding, protocol-scoped local-key digest comparison, operating-system credential storage for upstream/local/mapping secrets and custom dictionaries, no prompt fields in activity records, bounded and privacy-scanned activity metadata, AES-256-GCM mapping records with profile/token/kind AAD, request-size/concurrency limits, no default CORS, a narrow header-forwarding allowlist, and outbound redirects disabled.

Desktop profiles persist in SQLite without secret values. Windows builds reload upstream keys, local keys, mapping keys, and custom dictionaries from Windows Credential Manager. Encrypted reversible mapping records persist in SQLite and are reloaded through the profile-scoped mapping backend after restart. These paths still require clean-machine Windows release verification before distribution.

Residual risks include detector false negatives, compromised endpoint, malicious local process with access to user credentials, and unsupported request paths.
