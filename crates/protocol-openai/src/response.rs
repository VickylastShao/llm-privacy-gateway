use serde_json::Value;
use session_map::{MappingBackend, PseudonymMap};

use crate::OpenAiProtocolError;

/// Restores locally generated pseudonyms in non-streaming Chat Completions output.
pub fn restore_chat_response<B: MappingBackend>(
    response: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    let Some(choices) = response.get_mut("choices").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for choice in choices {
        if let Some(message) = choice.get_mut("message") {
            restore_string_field(message, "content", map)?;
            // `reasoning_content` is a provider extension used by DeepSeek-compatible models.
            restore_string_field(message, "reasoning_content", map)?;
        }
    }
    Ok(())
}

/// Restores approved text output fields from the OpenAI Responses API.
pub fn restore_responses_response<B: MappingBackend>(
    response: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    restore_string_field(response, "output_text", map)?;
    let Some(items) = response.get_mut("output").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for item in items {
        let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        for part in content {
            if matches!(
                part.get("type").and_then(Value::as_str),
                Some("output_text")
            ) {
                restore_string_field(part, "text", map)?;
            }
        }
    }
    Ok(())
}

fn restore_string_field<B: MappingBackend>(
    value: &mut Value,
    key: &str,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    let Some(field) = value.get_mut(key) else {
        return Ok(());
    };
    let Some(text) = field.as_str() else {
        return Ok(());
    };
    let restored = map
        .restore_text(text)
        .map_err(|_| OpenAiProtocolError::Privacy)?;
    *field = Value::String(restored);
    Ok(())
}
