use privacy_core::{Policy, PrivacyEngine};
use serde_json::Value;
use session_map::Pseudonymizer;

use crate::OpenAiProtocolError;

pub fn transform_responses_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    transform_string_field(request, "instructions", engine, policy, map)?;
    if let Some(input) = request.get_mut("input") {
        transform_input(input, engine, policy, map)?;
    }
    Ok(())
}

fn transform_input<P: Pseudonymizer>(
    input: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    match input {
        Value::String(text) => {
            *text = engine
                .transform(text, policy, map)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(items) => {
            for item in items {
                if let Some(content) = item.get_mut("content") {
                    transform_content(content, engine, policy, map)?;
                }
                if matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("function_call_output")
                ) {
                    transform_string_field(item, "output", engine, policy, map)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn transform_content<P: Pseudonymizer>(
    content: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    match content {
        Value::String(text) => {
            *text = engine
                .transform(text, policy, map)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(parts) => {
            for part in parts {
                if matches!(part.get("type").and_then(Value::as_str), Some("input_text")) {
                    transform_string_field(part, "text", engine, policy, map)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn transform_string_field<P: Pseudonymizer>(
    object: &mut Value,
    key: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    let Some(text_value) = object.get_mut(key) else {
        return Ok(());
    };
    let Some(text) = text_value.as_str() else {
        return Ok(());
    };
    let transformed = engine
        .transform(text, policy, map)
        .map_err(|_| OpenAiProtocolError::Privacy)?;
    *text_value = Value::String(transformed.outbound_text().to_owned());
    Ok(())
}
