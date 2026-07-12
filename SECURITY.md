# Security policy

Report vulnerabilities privately to the maintainer. Do not include production keys, prompts, documents, or mapping files in reports.

The gateway must bind only to `127.0.0.1` or `::1`. Upstream credentials belong in the operating-system credential store; they must not be written to profiles, activity rows, logs, or crash reports. Mapping keys and plaintext mappings must remain local.
