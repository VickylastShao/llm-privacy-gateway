# Threat model

Assets: upstream API keys, local API keys, mapping keys, plaintext prompts, original sensitive entities, and transformed responses.

Controls: loopback binding, protocol-scoped local-key digest comparison, OS credential storage abstraction, no prompt fields in activity records, AES-256-GCM mapping records with profile/token/kind AAD, request-size/concurrency limits, no default CORS, and outbound redirects disabled.

Residual risks include detector false negatives, compromised endpoint, malicious local process with access to user credentials, and unsupported request paths.
