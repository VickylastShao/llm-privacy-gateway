use privacy_core::{Policy, PrivacyEngine, RequestPrivacyReport, StaticDictionary};
use protocol_anthropic::{
    restore_messages_response_raw, transform_messages_request, transform_messages_request_raw,
    transform_messages_request_raw_with_report,
};
use serde_json::json;
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

#[test]
fn signed_thinking_blocks_remain_byte_equivalent_while_tool_results_transform() {
    let engine = PrivacyEngine::new(
        Vec::new(),
        StaticDictionary::new(["Alpha Project"])
            .unwrap_or_else(|error| panic!("dictionary: {error}")),
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [8; 32], MemoryBackend::default());
    let mut request = json!({"system":"Alpha Project","messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"Alpha Project","signature":"opaque-signature"},{"type":"tool_result","tool_use_id":"Alpha Project","content":"Alpha Project"},{"type":"tool_use","id":"Alpha Project","name":"read","input":"Alpha Project"}]}]});
    let signed = request["messages"][0]["content"][0].clone();
    transform_messages_request(&mut request, &engine, &Policy::strict(), &mut map)
        .unwrap_or_else(|error| panic!("transform: {error}"));
    assert_ne!(request["system"], "Alpha Project");
    assert_eq!(request["messages"][0]["content"][0], signed);
    assert_ne!(
        request["messages"][0]["content"][1]["content"],
        "Alpha Project"
    );
    assert_ne!(
        request["messages"][0]["content"][2]["input"],
        "Alpha Project"
    );
    assert_eq!(request["messages"][0]["content"][2]["id"], "Alpha Project");
}

#[test]
fn raw_adapter_keeps_signed_thinking_block_bytes_while_transforming_tool_result() {
    let engine = PrivacyEngine::new(
        Vec::new(),
        StaticDictionary::new(["Alpha Project"])
            .unwrap_or_else(|error| panic!("dictionary: {error}")),
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [9; 32], MemoryBackend::default());
    let signed = r#"{ "type" : "thinking", "thinking" : "opaque", "signature" : "sig==" }"#;
    let request = format!(
        r#"{{"model":"test","messages":[{{"role":"assistant","content":[{signed},{{"type":"tool_result","tool_use_id":"u1","content":"Alpha Project"}}]}}]}}"#
    );

    let output = transform_messages_request_raw(&request, &engine, &Policy::strict(), &mut map)
        .unwrap_or_else(|error| panic!("transform: {error}"));

    assert!(output.contains(signed));
    assert!(!output.contains("\"content\":\"Alpha Project\""));
}

#[test]
fn raw_response_adapter_restores_text_and_keeps_signed_thinking_bytes() {
    let mut map = PseudonymMap::new(Uuid::nil(), [18; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let signed = r#"{ "type" : "thinking", "thinking" : "opaque", "signature" : "sig==" }"#;
    let response = format!(r#"{{"content":[{signed},{{"type":"text","text":"{token}"}}]}}"#);

    let output = restore_messages_response_raw(&response, &map)
        .unwrap_or_else(|error| panic!("restore: {error}"));

    assert!(output.contains(signed));
    assert!(output.contains("alice@example.test"));
    assert!(!output.contains(&token));
}

#[test]
fn raw_adapter_aggregates_report_without_reserializing_signed_thinking() {
    let engine = PrivacyEngine::new(
        Vec::new(),
        StaticDictionary::new(["Alpha Project"])
            .unwrap_or_else(|error| panic!("dictionary: {error}")),
    );
    let mut map = PseudonymMap::new(Uuid::nil(), [19; 32], MemoryBackend::default());
    let signed = r#"{ "type" : "thinking", "thinking" : "opaque", "signature" : "sig==" }"#;
    let request = format!(
        r#"{{"system":"Alpha Project","messages":[{{"role":"assistant","content":[{signed},{{"type":"tool_result","tool_use_id":"u1","content":"Alpha Project"}}]}}]}}"#
    );
    let mut report = RequestPrivacyReport::default();

    let output = transform_messages_request_raw_with_report(
        &request,
        &engine,
        &Policy::strict(),
        &mut map,
        &mut report,
    )
    .unwrap_or_else(|error| panic!("transform: {error}"));

    assert!(output.contains(signed));
    assert_eq!(report.category_counts().get("custom"), Some(&2));
}
