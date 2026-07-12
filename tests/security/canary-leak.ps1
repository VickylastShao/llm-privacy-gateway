$ErrorActionPreference = "Stop"
$env:PATH = "$HOME/.cargo/bin;$env:PATH"
cargo test -p activity-store --test canary_leak --locked
cargo test -p gateway --test security_shell --locked
