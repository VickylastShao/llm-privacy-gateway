use std::collections::HashMap;

use serde_json::Value;
use session_map::{MapError, MappingBackend, PseudonymMap};

use crate::{OpenAiProtocolError, SseDecoder};

const PREFIX: &str = "__LPG_";

#[derive(Debug, Default)]
pub struct StreamRestorer {
    pending: String,
}

#[derive(Debug, Default)]
pub struct OpenAiSseTransformer {
    decoder: SseDecoder,
    restorers: EventRestorers,
    utf8_tail: Vec<u8>,
}

#[derive(Debug, Default)]
struct EventRestorers {
    streams: HashMap<String, StreamRestorer>,
}

impl EventRestorers {
    fn stream(&mut self, key: String) -> &mut StreamRestorer {
        self.streams.entry(key).or_default()
    }

    fn has_pending(self) -> bool {
        self.streams
            .into_values()
            .any(|restorer| !restorer.finish().is_empty())
    }
}

impl OpenAiSseTransformer {
    pub fn push_bytes<B: MappingBackend>(
        &mut self,
        bytes: &[u8],
        map: &PseudonymMap<B>,
    ) -> Result<Vec<u8>, OpenAiProtocolError> {
        self.utf8_tail.extend_from_slice(bytes);
        let text = match std::str::from_utf8(&self.utf8_tail) {
            Ok(text) => {
                let owned = text.to_owned();
                self.utf8_tail.clear();
                owned
            }
            Err(error) if error.error_len().is_none() => {
                let valid = std::str::from_utf8(&self.utf8_tail[..error.valid_up_to()])
                    .map_err(|_| OpenAiProtocolError::InvalidSseJson)?
                    .to_owned();
                let tail = self.utf8_tail[error.valid_up_to()..].to_vec();
                self.utf8_tail = tail;
                valid
            }
            Err(_) => return Err(OpenAiProtocolError::InvalidSseJson),
        };
        let events = self.decoder.push(&text)?;
        let mut output = String::new();
        for event in events {
            output.push_str(&transform_event(event, &mut self.restorers, map)?);
        }
        Ok(output.into_bytes())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, OpenAiProtocolError> {
        self.decoder.finish()?;
        if !self.utf8_tail.is_empty() || std::mem::take(&mut self.restorers).has_pending() {
            return Err(OpenAiProtocolError::IncompletePlaceholder);
        }
        Ok(Vec::new())
    }
}

impl StreamRestorer {
    pub fn push<B: MappingBackend>(
        &mut self,
        chunk: &str,
        map: &PseudonymMap<B>,
    ) -> Result<String, MapError> {
        self.pending.push_str(chunk);
        let mut output = String::new();
        loop {
            let Some(start) = self.pending.find(PREFIX) else {
                let keep = suffix_prefix_length(&self.pending);
                let split = self.pending.len() - keep;
                output.push_str(&self.pending[..split]);
                self.pending = self.pending[split..].to_owned();
                return Ok(output);
            };
            output.push_str(&self.pending[..start]);
            let after_prefix = start + PREFIX.len();
            let Some(relative_end) = self.pending[after_prefix..].find("__") else {
                self.pending = self.pending[start..].to_owned();
                return Ok(output);
            };
            let end = after_prefix + relative_end + 2;
            let token = &self.pending[start..end];
            output.push_str(&map.restore_text(token)?);
            self.pending = self.pending[end..].to_owned();
        }
    }

    #[must_use]
    pub fn finish(mut self) -> String {
        std::mem::take(&mut self.pending)
    }
}

fn suffix_prefix_length(value: &str) -> usize {
    (1..PREFIX.len())
        .rev()
        .find(|&length| value.ends_with(&PREFIX[..length]))
        .unwrap_or(0)
}

pub fn restore_openai_sse<B: MappingBackend>(
    input: &str,
    map: &PseudonymMap<B>,
) -> Result<String, OpenAiProtocolError> {
    let mut decoder = SseDecoder::default();
    let events = decoder.push(input)?;
    let mut restorers = EventRestorers::default();
    let mut output = String::new();
    for event in events {
        output.push_str(&transform_event(event, &mut restorers, map)?);
    }
    decoder.finish()?;
    if restorers.has_pending() {
        return Err(OpenAiProtocolError::IncompletePlaceholder);
    }
    Ok(output)
}

fn transform_event<B: MappingBackend>(
    event: crate::SseEvent,
    restorers: &mut EventRestorers,
    map: &PseudonymMap<B>,
) -> Result<String, OpenAiProtocolError> {
    if event.data == "[DONE]" {
        return Ok("data: [DONE]\n\n".to_owned());
    }
    let mut value: Value =
        serde_json::from_str(&event.data).map_err(|_| OpenAiProtocolError::InvalidSseJson)?;
    restore_event_text(&mut value, restorers, map)?;
    let mut output = String::new();
    if let Some(name) = event.event {
        output.push_str("event: ");
        output.push_str(&name);
        output.push('\n');
    }
    output.push_str("data: ");
    output
        .push_str(&serde_json::to_string(&value).map_err(|_| OpenAiProtocolError::InvalidSseJson)?);
    output.push_str("\n\n");
    Ok(output)
}

fn restore_event_text<B: MappingBackend>(
    value: &mut Value,
    restorers: &mut EventRestorers,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    if let Some(response) = value.get_mut("response") {
        crate::restore_responses_response(response, map)?;
    }
    if let Some(item) = value.get_mut("item") {
        crate::response::restore_responses_item(item, map)?;
    }
    if let Some(part) = value.get_mut("part") {
        crate::response::restore_responses_part(part, map)?;
    }
    if let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) {
        for (choice_position, choice) in choices.iter_mut().enumerate() {
            let choice_key = choice
                .get("index")
                .map(value_key_component)
                .unwrap_or_else(|| choice_position.to_string());
            if let Some(delta) = choice.get_mut("delta") {
                restore_delta_string(
                    delta,
                    "content",
                    restorers,
                    format!("chat:{choice_key}:content"),
                    map,
                )?;
                restore_delta_string(
                    delta,
                    "reasoning_content",
                    restorers,
                    format!("chat:{choice_key}:reasoning"),
                    map,
                )?;
                restore_delta_string(
                    delta,
                    "refusal",
                    restorers,
                    format!("chat:{choice_key}:refusal"),
                    map,
                )?;
                if let Some(tool_calls) = delta.get_mut("tool_calls").and_then(Value::as_array_mut)
                {
                    for (tool_position, tool_call) in tool_calls.iter_mut().enumerate() {
                        let tool_key = tool_call
                            .get("index")
                            .or_else(|| tool_call.get("id"))
                            .map(value_key_component)
                            .unwrap_or_else(|| tool_position.to_string());
                        if let Some(function) = tool_call.get_mut("function") {
                            restore_delta_string(
                                function,
                                "arguments",
                                restorers,
                                format!("chat:{choice_key}:tool:{tool_key}:arguments"),
                                map,
                            )?;
                        }
                    }
                }
            }
        }
    }
    let root_key = response_delta_key(value);
    if let Some(delta_value) = value.get_mut("delta") {
        let Some(delta) = delta_value.as_str() else {
            return Ok(());
        };
        let restored = restorers
            .stream(root_key)
            .push(delta, map)
            .map_err(|_| OpenAiProtocolError::Privacy)?;
        *delta_value = Value::String(restored);
    }
    Ok(())
}

fn restore_delta_string<B: MappingBackend>(
    delta: &mut Value,
    key: &str,
    restorers: &mut EventRestorers,
    stream_key: String,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    let Some(value) = delta.get_mut(key) else {
        return Ok(());
    };
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    let restored = restorers
        .stream(stream_key)
        .push(text, map)
        .map_err(|_| OpenAiProtocolError::Privacy)?;
    *value = Value::String(restored);
    Ok(())
}

fn response_delta_key(value: &Value) -> String {
    let mut components = vec![
        value
            .get("type")
            .map(value_key_component)
            .unwrap_or_else(|| "unknown".to_owned()),
    ];
    for field in ["output_index", "item_id", "content_index", "summary_index"] {
        components.push(
            value
                .get(field)
                .map(value_key_component)
                .unwrap_or_else(|| "-".to_owned()),
        );
    }
    format!("responses:{}", components.join(":"))
}

fn value_key_component(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}
