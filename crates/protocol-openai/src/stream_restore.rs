use serde_json::Value;
use session_map::{MapError, MappingBackend, PseudonymMap};

use crate::{OpenAiProtocolError, SseDecoder};

const PREFIX: &str = "__LPG_";

#[derive(Debug, Default)]
pub struct StreamRestorer {
    pending: String,
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
    let mut restorer = StreamRestorer::default();
    let mut output = String::new();
    for event in events {
        if event.data == "[DONE]" {
            output.push_str("data: [DONE]\n\n");
            continue;
        }
        let mut value: Value =
            serde_json::from_str(&event.data).map_err(|_| OpenAiProtocolError::InvalidSseJson)?;
        restore_event_text(&mut value, &mut restorer, map)?;
        if let Some(name) = event.event {
            output.push_str("event: ");
            output.push_str(&name);
            output.push('\n');
        }
        output.push_str("data: ");
        output.push_str(
            &serde_json::to_string(&value).map_err(|_| OpenAiProtocolError::InvalidSseJson)?,
        );
        output.push_str("\n\n");
    }
    let tail = restorer.finish();
    if !tail.is_empty() {
        return Err(OpenAiProtocolError::IncompletePlaceholder);
    }
    Ok(output)
}

fn restore_event_text<B: MappingBackend>(
    value: &mut Value,
    restorer: &mut StreamRestorer,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    if let Some(choices) = value.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(content_value) = choice
                .get_mut("delta")
                .and_then(|delta| delta.get_mut("content"))
            {
                let Some(content) = content_value.as_str() else {
                    continue;
                };
                let restored = restorer
                    .push(content, map)
                    .map_err(|_| OpenAiProtocolError::Privacy)?;
                *content_value = Value::String(restored);
            }
        }
    }
    if let Some(delta_value) = value.get_mut("delta") {
        let Some(delta) = delta_value.as_str() else {
            return Ok(());
        };
        let restored = restorer
            .push(delta, map)
            .map_err(|_| OpenAiProtocolError::Privacy)?;
        *delta_value = Value::String(restored);
    }
    Ok(())
}
