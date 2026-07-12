use protocol_openai::{OpenAiSseTransformer, SseDecoder, StreamRestorer, restore_openai_sse};
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

#[test]
fn transformer_accepts_every_utf8_byte_split() {
    let mut map = PseudonymMap::new(Uuid::nil(), [9; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let source =
        format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"你好 {token}\"}}}}]}}\n\n");
    for split in 0..=source.len() {
        let mut transformer = OpenAiSseTransformer::default();
        let mut output = transformer
            .push_bytes(&source.as_bytes()[..split], &map)
            .unwrap_or_else(|error| panic!("first split: {error}"));
        output.extend(
            transformer
                .push_bytes(&source.as_bytes()[split..], &map)
                .unwrap_or_else(|error| panic!("second split: {error}")),
        );
        output.extend(
            transformer
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error}")),
        );
        let text = String::from_utf8(output).unwrap_or_else(|error| panic!("UTF-8: {error}"));
        assert!(text.contains("你好 alice@example.test"), "split {split}");
    }
}

#[test]
fn transformer_fails_closed_on_an_unterminated_event() {
    let map = PseudonymMap::new(Uuid::nil(), [1; 32], MemoryBackend::default());
    let mut transformer = OpenAiSseTransformer::default();
    transformer
        .push_bytes(b"data: {\"choices\":[]}", &map)
        .unwrap_or_else(|error| panic!("partial event: {error}"));
    assert!(transformer.finish().is_err());
}

#[test]
fn restores_provider_reasoning_content_without_emitting_partial_placeholders() {
    let mut map = PseudonymMap::new(Uuid::nil(), [17; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let split = token.len() / 2;
    let source = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"reasoning_content\":\"{}\"}}}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{\"reasoning_content\":\"{}\"}}}}]}}\n\n",
        &token[..split],
        &token[split..]
    );

    let output =
        restore_openai_sse(&source, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    assert!(output.contains("alice@example.test"));
    assert!(!output.contains("__LPG_"));
}
