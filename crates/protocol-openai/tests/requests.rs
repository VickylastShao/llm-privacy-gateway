use privacy_core::{Policy, PrivacyEngine, RequestPrivacyReport, StaticDictionary};
use protocol_openai::{
    restore_chat_response, transform_chat_request, transform_chat_request_with_report,
    transform_responses_request, transform_responses_request_with_report,
};
use serde_json::json;
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

fn setup() -> (PrivacyEngine, PseudonymMap<MemoryBackend>) {
    (
        PrivacyEngine::new(
            Vec::new(),
            StaticDictionary::new(["Alpha Project"])
                .unwrap_or_else(|error| panic!("dictionary: {error}")),
        ),
        PseudonymMap::new(Uuid::nil(), [1; 32], MemoryBackend::default()),
    )
}

#[test]
fn chat_mutates_only_whitelisted_text_and_tool_arguments() {
    let (engine, mut map) = setup();
    let mut request = json!({"messages":[{"role":"user","content":[{"type":"text","text":"Alpha Project"},{"type":"image_url","image_url":{"url":"data:image/png;base64,Alpha Project"}}],"tool_calls":[{"id":"call_1","function":{"name":"lookup","arguments":"Alpha Project"}}]}],"tools":[{"type":"function","function":{"name":"lookup","parameters":{"description":"Alpha Project"}}}],"user":"Alpha Project"});
    transform_chat_request(&mut request, &engine, &Policy::strict(), &mut map)
        .unwrap_or_else(|error| panic!("transform: {error}"));
    assert_ne!(
        request["messages"][0]["content"][0]["text"],
        "Alpha Project"
    );
    assert_ne!(
        request["messages"][0]["tool_calls"][0]["function"]["arguments"],
        "Alpha Project"
    );
    assert_eq!(
        request["messages"][0]["content"][1]["image_url"]["url"],
        "data:image/png;base64,Alpha Project"
    );
    assert_eq!(
        request["tools"][0]["function"]["parameters"]["description"],
        "Alpha Project"
    );
    assert_eq!(request["user"], "Alpha Project");
}

#[test]
fn responses_preserves_ids_and_encrypted_reasoning() {
    let (engine, mut map) = setup();
    let mut request = json!({"instructions":"Alpha Project","input":[{"type":"message","content":[{"type":"input_text","text":"Alpha Project"},{"type":"input_image","file_id":"Alpha Project"}]},{"type":"function_call_output","call_id":"Alpha Project","output":"Alpha Project"}],"reasoning":{"encrypted_content":"Alpha Project"}});
    transform_responses_request(&mut request, &engine, &Policy::strict(), &mut map)
        .unwrap_or_else(|error| panic!("transform: {error}"));
    assert_ne!(request["instructions"], "Alpha Project");
    assert_ne!(request["input"][0]["content"][0]["text"], "Alpha Project");
    assert_ne!(request["input"][1]["output"], "Alpha Project");
    assert_eq!(
        request["input"][0]["content"][1]["file_id"],
        "Alpha Project"
    );
    assert_eq!(request["input"][1]["call_id"], "Alpha Project");
    assert_eq!(request["reasoning"]["encrypted_content"], "Alpha Project");
}

#[test]
fn chat_response_restores_message_and_reasoning_text() {
    let mut map = PseudonymMap::new(Uuid::nil(), [15; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let mut response = json!({
        "choices": [{
            "message": {
                "content": token,
                "reasoning_content": format!("repeat {token}")
            }
        }]
    });

    restore_chat_response(&mut response, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    assert_eq!(
        response["choices"][0]["message"]["content"],
        "alice@example.test"
    );
    assert_eq!(
        response["choices"][0]["message"]["reasoning_content"],
        "repeat alice@example.test"
    );
}

#[test]
fn openai_adapters_aggregate_all_approved_fields_into_one_report() {
    let (engine, mut map) = setup();
    let mut chat = json!({"messages":[{"role":"user","content":"Alpha Project","tool_calls":[{"function":{"arguments":"Alpha Project"}}]}]});
    let mut chat_report = RequestPrivacyReport::default();
    transform_chat_request_with_report(
        &mut chat,
        &engine,
        &Policy::strict(),
        &mut map,
        &mut chat_report,
    )
    .unwrap_or_else(|error| panic!("chat transform: {error}"));
    assert_eq!(chat_report.category_counts().get("custom"), Some(&2));

    let mut responses = json!({"instructions":"Alpha Project","input":[{"type":"message","content":[{"type":"input_text","text":"Alpha Project"}]},{"type":"function_call_output","output":"Alpha Project"}]});
    let mut responses_report = RequestPrivacyReport::default();
    transform_responses_request_with_report(
        &mut responses,
        &engine,
        &Policy::strict(),
        &mut map,
        &mut responses_report,
    )
    .unwrap_or_else(|error| panic!("responses transform: {error}"));
    assert_eq!(responses_report.category_counts().get("custom"), Some(&3));
}
