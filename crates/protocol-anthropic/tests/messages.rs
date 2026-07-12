use privacy_core::{Policy, PrivacyEngine, StaticDictionary};
use protocol_anthropic::transform_messages_request;
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
