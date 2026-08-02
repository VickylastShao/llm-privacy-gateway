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
            if let Some(tool_calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
                for tool_call in tool_calls {
                    if let Some(function) = tool_call.get_mut("function") {
                        restore_string_field(function, "arguments", map)?;
                    }
                }
            }
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
        restore_responses_item(item, map)?;
    }
    Ok(())
}

pub(crate) fn restore_responses_item<B: MappingBackend>(
    item: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    if matches!(
        item.get("type").and_then(Value::as_str),
        Some("function_call")
    ) {
        restore_string_field(item, "arguments", map)?;
    }
    if matches!(
        item.get("type").and_then(Value::as_str),
        Some("function_call_output")
    ) && let Some(output) = item.get_mut("output")
    {
        restore_function_output(output, map)?;
    }
    let Some(content) = item.get_mut("content").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for part in content {
        restore_responses_part(part, map)?;
    }
    Ok(())
}

fn restore_function_output<B: MappingBackend>(
    output: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    match output {
        Value::String(_) => restore_value_string(output, map),
        Value::Array(parts) => {
            for part in parts {
                if part.get("type").and_then(Value::as_str) == Some("input_text") {
                    restore_string_field(part, "text", map)?;
                }
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn restore_value_string<B: MappingBackend>(
    value: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    *value = Value::String(
        map.restore_text(text)
            .map_err(|_| OpenAiProtocolError::Privacy)?,
    );
    Ok(())
}

pub(crate) fn restore_responses_part<B: MappingBackend>(
    part: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), OpenAiProtocolError> {
    if matches!(
        part.get("type").and_then(Value::as_str),
        Some("output_text")
    ) {
        restore_string_field(part, "text", map)?;
    }
    Ok(())
}

pub(crate) fn restore_string_field<B: MappingBackend>(
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
