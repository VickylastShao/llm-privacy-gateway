use privacy_core::{Policy, PrivacyEngine};
use serde_json::Value;
use session_map::Pseudonymizer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnthropicProtocolError {
    #[error("privacy transformation failed")]
    Privacy,
}

pub fn transform_messages_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), AnthropicProtocolError> {
    if let Some(system) = request.get_mut("system") {
        transform_content(system, engine, policy, map)?;
    }
    if let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.get_mut("content") {
                transform_content(content, engine, policy, map)?;
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
) -> Result<(), AnthropicProtocolError> {
    match content {
        Value::String(text) => {
            *text = engine
                .transform(text, policy, map)
                .map_err(|_| AnthropicProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(blocks) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => transform_field(block, "text", engine, policy, map)?,
                    Some("tool_result") => transform_field(block, "content", engine, policy, map)?,
                    Some("tool_use") => transform_field(block, "input", engine, policy, map)?,
                    _ => {}
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn transform_field<P: Pseudonymizer>(
    block: &mut Value,
    key: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), AnthropicProtocolError> {
    let Some(value) = block.get_mut(key) else {
        return Ok(());
    };
    if let Some(text) = value.as_str() {
        *value = Value::String(
            engine
                .transform(text, policy, map)
                .map_err(|_| AnthropicProtocolError::Privacy)?
                .outbound_text()
                .to_owned(),
        );
    }
    Ok(())
}
