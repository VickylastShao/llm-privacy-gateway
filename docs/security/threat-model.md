# Threat model

Assets: upstream API keys, local API keys, mapping keys, plaintext prompts, original sensitive entities, and transformed responses.

Controls: loopback binding, protocol-scoped local-key digest comparison, an OS credential-storage abstraction, no prompt fields in activity records, AES-256-GCM mapping records with profile/token/kind AAD, request-size/concurrency limits, no default CORS, and outbound redirects disabled.

Current desktop limitation: the control plane starts its configured profiles from process memory. Cross-restart profile persistence and Windows Credential Manager wiring are release blockers; they must be completed and verified on clean Windows before the desktop application is represented as a persistent credential manager.

Residual risks include detector false negatives, compromised endpoint, malicious local process with access to user credentials, and unsupported request paths.
