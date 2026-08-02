use protocol_openai::{OpenAiSseTransformer, SseDecoder, StreamRestorer, restore_openai_sse};
use serde_json::json;
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

#[test]
fn chat_stream_restores_split_tool_arguments() {
    let mut map = PseudonymMap::new(Uuid::nil(), [18; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let split = token.len() / 2;
    let event = |arguments: &str| {
        format!(
            "data: {}\n\n",
            json!({
                "choices": [{
                    "index": 0,
                    "delta": {"tool_calls": [{
                        "index": 0,
                        "function": {"arguments": arguments}
                    }]}
                }]
            })
        )
    };
    let source = format!(
        "{}{}data: [DONE]\n\n",
        event(&format!(r#"{{"email":"{}"#, &token[..split])),
        event(&format!(r#"{}"}}"#, &token[split..]))
    );

    let output =
        restore_openai_sse(&source, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    let arguments = output
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| {
            serde_json::from_str::<serde_json::Value>(data)
                .unwrap_or_else(|error| panic!("event JSON: {error}"))["choices"][0]["delta"]
                ["tool_calls"][0]["function"]["arguments"]
                .as_str()
                .unwrap_or_else(|| panic!("arguments must be a string"))
                .to_owned()
        })
        .collect::<String>();
    assert_eq!(arguments, r#"{"email":"alice@example.test"}"#);
    assert!(!output.contains(&token));
}

#[test]
fn chat_stream_keeps_interleaved_choices_isolated() {
    let mut map = PseudonymMap::new(Uuid::nil(), [19; 32], MemoryBackend::default());
    let first = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("first map: {error}"));
    let second = map
        .pseudonymize("email", "bob@example.test")
        .unwrap_or_else(|error| panic!("second map: {error}"));
    let split = first.len() / 2;
    let event = |index: usize, content: &str| {
        format!(
            "data: {}\n\n",
            json!({"choices":[{"index":index,"delta":{"content":content}}]})
        )
    };
    let source = format!(
        "{}{}{}data: [DONE]\n\n",
        event(0, &first[..split]),
        event(1, &second),
        event(0, &first[split..]),
    );

    let output =
        restore_openai_sse(&source, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    assert!(output.contains("alice@example.test"));
    assert!(output.contains("bob@example.test"));
    assert!(!output.contains("__LPG_"));
}

#[test]
fn responses_stream_keeps_interleaved_output_items_isolated() {
    let mut map = PseudonymMap::new(Uuid::nil(), [20; 32], MemoryBackend::default());
    let first = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("first map: {error}"));
    let second = map
        .pseudonymize("email", "bob@example.test")
        .unwrap_or_else(|error| panic!("second map: {error}"));
    let split = first.len() / 2;
    let event = |output_index: usize, delta: &str| {
        format!(
            "event: response.output_text.delta\ndata: {}\n\n",
            json!({
                "type":"response.output_text.delta",
                "output_index":output_index,
                "content_index":0,
                "delta":delta
            })
        )
    };
    let source = format!(
        "{}{}{}",
        event(0, &first[..split]),
        event(1, &second),
        event(0, &first[split..]),
    );

    let output =
        restore_openai_sse(&source, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    assert!(output.contains("alice@example.test"));
    assert!(output.contains("bob@example.test"));
    assert!(!output.contains("__LPG_"));
}

#[test]
fn responses_stream_restores_completed_items_parts_and_responses() {
    let mut map = PseudonymMap::new(Uuid::nil(), [21; 32], MemoryBackend::default());
    let token = map
        .pseudonymize("email", "alice@example.test")
        .unwrap_or_else(|error| panic!("map: {error}"));
    let event = |name: &str, value: serde_json::Value| format!("event: {name}\ndata: {value}\n\n");
    let source = format!(
        "{}{}{}",
        event(
            "response.output_item.done",
            json!({
                "type": "response.output_item.done",
                "item": {"type": "function_call", "arguments": format!(r#"{{\"email\":\"{token}\"}}"#)}
            }),
        ),
        event(
            "response.content_part.done",
            json!({
                "type": "response.content_part.done",
                "part": {"type": "output_text", "text": format!("Contact {token}")}
            }),
        ),
        event(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "output_text": format!("Contact {token}"),
                    "output": [{
                        "type": "message",
                        "content": [{"type": "output_text", "text": format!("Contact {token}")}]
                    }]
                }
            }),
        ),
    );

    let output =
        restore_openai_sse(&source, &map).unwrap_or_else(|error| panic!("restore: {error}"));

    assert_eq!(output.matches("alice@example.test").count(), 4);
    assert!(!output.contains(&token));
}
