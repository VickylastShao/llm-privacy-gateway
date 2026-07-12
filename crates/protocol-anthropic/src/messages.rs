use privacy_core::{Policy, PrivacyEngine};
use serde_json::{Value, value::RawValue};
use session_map::Pseudonymizer;
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnthropicProtocolError {
    #[error("privacy transformation failed")]
    Privacy,
    #[error("invalid Anthropic JSON")]
    Json,
}

/// Transforms only approved Anthropic content fields while carrying all other JSON values as
/// `RawValue`. In particular, signed thinking blocks are never parsed or reserialized.
pub fn transform_messages_request_raw<P: Pseudonymizer>(
    input: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<String, AnthropicProtocolError> {
    let mut request: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(input).map_err(|_| AnthropicProtocolError::Json)?;

    if let Some(system) = request.remove("system") {
        request.insert(
            "system".to_owned(),
            raw_value(transform_raw_content(system.get(), engine, policy, map)?)?,
        );
    }
    if let Some(messages) = request.remove("messages") {
        let messages: Vec<Box<RawValue>> =
            serde_json::from_str(messages.get()).map_err(|_| AnthropicProtocolError::Json)?;
        let mut transformed = Vec::with_capacity(messages.len());
        for message in messages {
            transformed.push(transform_raw_message(&message, engine, policy, map)?);
        }
        request.insert(
            "messages".to_owned(),
            raw_value(
                serde_json::to_string(&transformed).map_err(|_| AnthropicProtocolError::Json)?,
            )?,
        );
    }
    serde_json::to_string(&request).map_err(|_| AnthropicProtocolError::Json)
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

fn transform_raw_message<P: Pseudonymizer>(
    message: &RawValue,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<Box<RawValue>, AnthropicProtocolError> {
    let mut message: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(message.get()).map_err(|_| AnthropicProtocolError::Json)?;
    if let Some(content) = message.remove("content") {
        message.insert(
            "content".to_owned(),
            raw_value(transform_raw_content(content.get(), engine, policy, map)?)?,
        );
    }
    raw_value(serde_json::to_string(&message).map_err(|_| AnthropicProtocolError::Json)?)
}

fn transform_raw_content<P: Pseudonymizer>(
    content: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<String, AnthropicProtocolError> {
    let value: Value = serde_json::from_str(content).map_err(|_| AnthropicProtocolError::Json)?;
    match value {
        Value::String(text) => serde_json::to_string(
            engine
                .transform(&text, policy, map)
                .map_err(|_| AnthropicProtocolError::Privacy)?
                .outbound_text(),
        )
        .map_err(|_| AnthropicProtocolError::Json),
        Value::Array(_) => {
            let blocks: Vec<Box<RawValue>> =
                serde_json::from_str(content).map_err(|_| AnthropicProtocolError::Json)?;
            let mut transformed = Vec::with_capacity(blocks.len());
            for block in blocks {
                transformed.push(transform_raw_block(block, engine, policy, map)?);
            }
            serde_json::to_string(&transformed).map_err(|_| AnthropicProtocolError::Json)
        }
        _ => Ok(content.to_owned()),
    }
}

fn transform_raw_block<P: Pseudonymizer>(
    block: Box<RawValue>,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<Box<RawValue>, AnthropicProtocolError> {
    let mut value: Value =
        serde_json::from_str(block.get()).map_err(|_| AnthropicProtocolError::Json)?;
    match value.get("type").and_then(Value::as_str) {
        Some("text") => transform_field(&mut value, "text", engine, policy, map)?,
        Some("tool_result") => transform_field(&mut value, "content", engine, policy, map)?,
        Some("tool_use") => transform_field(&mut value, "input", engine, policy, map)?,
        // Thinking, redacted_thinking, image and unknown blocks must remain raw. Anthropic
        // signatures are tied to the original serialized block rather than its JSON meaning.
        _ => return Ok(block),
    }
    raw_value(serde_json::to_string(&value).map_err(|_| AnthropicProtocolError::Json)?)
}

fn raw_value(value: String) -> Result<Box<RawValue>, AnthropicProtocolError> {
    RawValue::from_string(value).map_err(|_| AnthropicProtocolError::Json)
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
