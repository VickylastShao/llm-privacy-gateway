$ErrorActionPreference = "Stop"
$env:PATH = "$HOME/.cargo/bin;$env:PATH"
cargo test -p activity-store --test canary_leak --locked
cargo test -p gateway --test security_shell transformed_canary_never_reaches_mock_upstream_in_plaintext --locked
