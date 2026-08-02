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

#[test]
fn transformer_restores_crlf_delimited_multiline_data_and_preserves_ping_bytes() {
    let mut map = PseudonymMap::new(Uuid::nil(), [3; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|e| panic!("map: {e}"));
    let source = format!(
        ": keepalive\r\n\r\nevent: content_block_delta\r\ndata: {{\"delta\":\r\ndata: {{\"text\":\"{token}\"}}}}\r\n\r\n"
    );

    let output = restore_anthropic_sse(&source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    assert!(output.starts_with(": keepalive\r\n\r\n"));
    assert!(output.contains("alice@example.test"));
    assert!(!output.contains(&token));
    assert!(output.ends_with("\r\n\r\n"));
}

#[test]
fn transformer_preserves_unknown_events_byte_for_byte() {
    let map = PseudonymMap::new(Uuid::nil(), [4; 32], MemoryBackend::default());
    let source = "event: message_start\r\ndata: { \"opaque\" : true }\r\nretry: 1000\r\n\r\n";

    let output = restore_anthropic_sse(source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    assert_eq!(output, source);
}

#[test]
fn transformer_preserves_non_text_content_delta_byte_for_byte() {
    let map = PseudonymMap::new(Uuid::nil(), [5; 32], MemoryBackend::default());
    let source =
        "event: content_block_delta\r\ndata: { \"delta\" : { \"thinking\" : \"opaque\" } }\r\n\r\n";

    let output = restore_anthropic_sse(source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    assert_eq!(output, source);
}

#[test]
fn transformer_buffers_input_json_delta_until_block_stop_then_restores_json() {
    let mut map = PseudonymMap::new(Uuid::nil(), [6; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|e| panic!("map: {e}"));
    let split = token.len() / 2;
    let first = format!(r#"{{"email":"{}"#, &token[..split]);
    let second = format!(r#"{}"}}"#, &token[split..]);
    let delta = |partial_json: String| {
        serde_json::json!({
            "type": "content_block_delta",
            "index": 1,
            "delta": {"type": "input_json_delta", "partial_json": partial_json}
        })
    };
    let source = format!(
        "event: content_block_delta\ndata: {}\n\nevent: content_block_delta\ndata: {}\n\nevent: content_block_stop\ndata: {{\"type\":\"content_block_stop\",\"index\":1}}\n\n",
        delta(first),
        delta(second),
    );

    let output = restore_anthropic_sse(&source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    assert_eq!(output.matches("input_json_delta").count(), 1);
    assert!(output.contains(r#"{\"email\":\"alice@example.test\"}"#));
    assert!(!output.contains(&token));
    assert!(output.contains("event: content_block_stop"));
}

#[test]
fn transformer_restores_nested_tool_input_in_content_block_start() {
    let mut map = PseudonymMap::new(Uuid::nil(), [8; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|e| panic!("map: {e}"));
    let source = format!(
        "event: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":1,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"lookup\",\"input\":{{\"contact\":{{\"email\":\"{token}\"}},\"enabled\":true}}}}}}\n\n"
    );

    let output = restore_anthropic_sse(&source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    let data = output
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .unwrap_or_else(|| panic!("event data"));
    let value: serde_json::Value =
        serde_json::from_str(data).unwrap_or_else(|e| panic!("event JSON: {e}"));
    assert_eq!(
        value["content_block"]["input"]["contact"]["email"],
        "alice@example.test"
    );
    assert_eq!(value["content_block"]["input"]["enabled"], true);
    assert!(!output.contains(&token));
}

#[test]
fn transformer_preserves_signed_thinking_content_block_start_byte_for_byte() {
    let map = PseudonymMap::new(Uuid::nil(), [9; 32], MemoryBackend::default());
    let source = "event: content_block_start\r\ndata: { \"type\" : \"content_block_start\", \"index\" : 0, \"content_block\" : { \"type\" : \"thinking\", \"thinking\" : \"opaque\", \"signature\" : \"signed\" } }\r\n\r\n";

    let output = restore_anthropic_sse(source, &map).unwrap_or_else(|e| panic!("restore: {e}"));

    assert_eq!(output, source);
}
