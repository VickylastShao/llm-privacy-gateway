use protocol_openai::{SseDecoder, StreamRestorer, restore_openai_sse};
use session_map::{MemoryBackend, PseudonymMap};
use uuid::Uuid;

#[test]
fn restoration_holds_partial_placeholders_across_every_ascii_split() {
    let mut map = PseudonymMap::new(Uuid::nil(), [5; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let full = format!("Contact {token} now");
    for split in 0..=full.len() {
        let mut restorer = StreamRestorer::default();
        let mut restored = restorer
            .push(&full[..split], &map)
            .unwrap_or_else(|error| panic!("first chunk: {error}"));
        restored.push_str(
            &restorer
                .push(&full[split..], &map)
                .unwrap_or_else(|error| panic!("second chunk: {error}")),
        );
        restored.push_str(&restorer.finish());
        assert_eq!(restored, "Contact alice@example.test now", "split {split}");
    }
}

#[test]
fn sse_decoder_supports_comments_crlf_multiline_and_unknown_events() {
    let mut decoder = SseDecoder::default();
    assert!(
        decoder
            .push(": comment\r\nevent: response.unknown\r\ndata: first\r\ndata: second\r\n")
            .unwrap_or_else(|_| panic!("frame parse"))
            .is_empty()
    );
    let events = decoder
        .push("\r\n")
        .unwrap_or_else(|_| panic!("frame parse"));
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.as_deref(), Some("response.unknown"));
    assert_eq!(events[0].data, "first\nsecond");
}

#[test]
fn openai_sse_restores_placeholder_split_across_delta_events() {
    let mut map = PseudonymMap::new(Uuid::nil(), [6; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let split = token.len() / 2;
    let input = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"Contact {}\"}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{\"content\":\"{} now\"}}}}]}}\n\ndata: [DONE]\n\n",
        &token[..split],
        &token[split..]
    );
    let output =
        restore_openai_sse(&input, &map).unwrap_or_else(|error| panic!("restore: {error}"));
    assert!(output.contains("Contact "));
    assert!(output.contains("alice@example.test now"));
    assert!(!output.contains(&token));
}
