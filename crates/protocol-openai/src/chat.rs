use privacy_core::{Policy, PrivacyEngine};
use serde_json::Value;
use session_map::Pseudonymizer;

use crate::OpenAiProtocolError;

pub fn transform_chat_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for message in messages {
        transform_message(message, engine, policy, map)?;
    }
    Ok(())
}

fn transform_message<P: Pseudonymizer>(
    message: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    if let Some(content) = message.get_mut("content") {
        transform_content(content, engine, policy, map)?;
    }
    if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
        for call in calls {
            if let Some(arguments_value) = call
                .get_mut("function")
                .and_then(|function| function.get_mut("arguments"))
            {
                let Some(arguments) = arguments_value.as_str() else {
                    continue;
                };
                let transformed = engine
                    .transform(arguments, policy, map)
                    .map_err(|_| OpenAiProtocolError::Privacy)?;
                *arguments_value = Value::String(transformed.outbound_text().to_owned());
            }
        }
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
                if matches!(
                    part.get("type").and_then(Value::as_str),
                    Some("text") | Some("input_text")
                ) && let Some(text_value) = part.get_mut("text")
                {
                    let Some(text) = text_value.as_str() else {
                        continue;
                    };
                    let transformed = engine
                        .transform(text, policy, map)
                        .map_err(|_| OpenAiProtocolError::Privacy)?;
                    *text_value = Value::String(transformed.outbound_text().to_owned());
                }
            }
        }
        _ => {}
    }
    Ok(())
}
