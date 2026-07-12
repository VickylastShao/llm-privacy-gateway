use protocol_anthropic::{AnthropicSseTransformer, restore_anthropic_sse};
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

#[test]
fn transformer_accepts_every_utf8_byte_split() {
    let mut map = PseudonymMap::new(Uuid::nil(), [2; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|e| panic!("map: {e}"));
    let source = format!(
        "event: content_block_delta\ndata: {{\"delta\":{{\"text\":\"你好 {token}\"}}}}\n\n"
    );
    for split in 0..=source.len() {
        let mut transformer = AnthropicSseTransformer::default();
        let mut output = transformer
            .push_bytes(&source.as_bytes()[..split], &map)
            .unwrap_or_else(|e| panic!("first: {e}"));
        output.extend(
            transformer
                .push_bytes(&source.as_bytes()[split..], &map)
                .unwrap_or_else(|e| panic!("second: {e}")),
        );
        output.extend(
            transformer
                .finish()
                .unwrap_or_else(|e| panic!("finish: {e}")),
        );
        assert!(
            String::from_utf8(output)
                .unwrap_or_else(|e| panic!("UTF-8: {e}"))
                .contains("你好 alice@example.test")
        );
    }
}
