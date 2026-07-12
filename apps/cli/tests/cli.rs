use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lpg-cli"))
}

#[test]
fn detect_hides_sensitive_values_unless_explicitly_requested() {
    let output = cli()
        .args(["detect", "email qa@example.test"])
        .output()
        .unwrap_or_else(|error| panic!("cli must run: {error}"));
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Email"));
    assert!(!stdout.contains("qa@example.test"));
}

#[test]
fn blocked_transform_exits_two_without_mapping_file() {
    let mapping = tempfile::NamedTempFile::new()
        .unwrap_or_else(|error| panic!("tempfile must work: {error}"));
    let output = cli()
        .args([
            "transform",
            "key=sk-test-abcdefghijklmnopqrstuvwxyz",
            "--mapping-file",
            mapping.path().to_str().unwrap_or(""),
            "--mapping-key",
            &"11".repeat(32),
        ])
        .output()
        .unwrap_or_else(|error| panic!("cli must run: {error}"));
    assert_eq!(output.status.code(), Some(2));
    assert!(std::fs::read(mapping.path()).unwrap_or_default().is_empty());
}

#[test]
fn transform_and_restore_use_an_encrypted_mapping_file() {
    let mapping = tempfile::NamedTempFile::new()
        .unwrap_or_else(|error| panic!("tempfile must work: {error}"));
    let mapping_path = mapping.path().to_str().unwrap_or("");
    let key = "22".repeat(32);
    let transformed = cli()
        .args([
            "transform",
            "email qa@example.test",
            "--mapping-file",
            mapping_path,
            "--mapping-key",
            &key,
        ])
        .output()
        .unwrap_or_else(|error| panic!("cli must run: {error}"));
    assert!(transformed.status.success());
    let transformed_json: serde_json::Value = serde_json::from_slice(&transformed.stdout)
        .unwrap_or_else(|error| panic!("JSON output must be valid: {error}"));
    let outbound = transformed_json["outbound_text"]
        .as_str()
        .unwrap_or_else(|| panic!("transform must provide outbound text"));
    assert_ne!(outbound, "email qa@example.test");
    assert!(
        !std::fs::read_to_string(mapping.path())
            .unwrap_or_default()
            .contains("qa@example.test")
    );

    let restored = cli()
        .args([
            "restore",
            outbound,
            "--mapping-file",
            mapping_path,
            "--mapping-key",
            &key,
        ])
        .output()
        .unwrap_or_else(|error| panic!("cli must run: {error}"));
    assert!(restored.status.success());
    assert!(String::from_utf8_lossy(&restored.stdout).contains("qa@example.test"));
}

#[test]
fn eval_reports_metrics_for_synthetic_fixtures() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../privacy-evals");
    let output = cli()
        .args(["eval", root.to_str().unwrap_or("")])
        .output()
        .unwrap_or_else(|error| panic!("cli must run: {error}"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("precision"));
}
