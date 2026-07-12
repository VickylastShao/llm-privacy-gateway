use protocol_anthropic::restore_anthropic_sse;
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

#[test]
fn restores_only_text_delta_and_preserves_thinking_event() {
    let mut map = PseudonymMap::new(Uuid::nil(), [7; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|e| panic!("map: {e}"));
    let split = token.len() / 2;
    let input = format!(
        "event: thinking_delta\ndata: {{\"delta\":{{\"thinking\":\"opaque\"}}}}\n\nevent: content_block_delta\ndata: {{\"delta\":{{\"text\":\"{}\"}}}}\n\nevent: content_block_delta\ndata: {{\"delta\":{{\"text\":\"{}\"}}}}\n\n",
        &token[..split],
        &token[split..]
    );
    let output = restore_anthropic_sse(&input, &map).unwrap_or_else(|e| panic!("restore: {e}"));
    assert!(output.contains("opaque"));
    assert!(output.contains("alice@example.test"));
    assert!(!output.contains(&token));
}
