use privacy_core::{Policy, PolicyAction, PrivacyEngine, RequestPrivacyReport};
use serde_json::{Value, value::RawValue};
use session_map::{MappingBackend, PseudonymMap, Pseudonymizer};
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
    transform_messages_request_raw_with_report(
        input,
        engine,
        policy,
        map,
        &mut RequestPrivacyReport::default(),
    )
}

pub fn transform_messages_request_raw_with_report<P: Pseudonymizer>(
    input: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<String, AnthropicProtocolError> {
    let mut request: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(input).map_err(|_| AnthropicProtocolError::Json)?;

    if let Some(system) = request.remove("system") {
        request.insert(
            "system".to_owned(),
            raw_value(transform_raw_content(
                system.get(),
                engine,
                policy,
                map,
                report,
            )?)?,
        );
    }
    if let Some(messages) = request.remove("messages") {
        let messages: Vec<Box<RawValue>> =
            serde_json::from_str(messages.get()).map_err(|_| AnthropicProtocolError::Json)?;
        let mut transformed = Vec::with_capacity(messages.len());
        for message in messages {
            transformed.push(transform_raw_message(
                &message, engine, policy, map, report,
            )?);
        }
        request.insert(
            "messages".to_owned(),
            raw_value(
                serde_json::to_string(&transformed).map_err(|_| AnthropicProtocolError::Json)?,
            )?,
        );
    }
    enforce_content_coverage(policy, report)?;
    serde_json::to_string(&request).map_err(|_| AnthropicProtocolError::Json)
}

/// Restores pseudonyms in response text blocks while leaving signed thinking blocks untouched.
pub fn restore_messages_response_raw<B: MappingBackend>(
    input: &str,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicProtocolError> {
    let mut response: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(input).map_err(|_| AnthropicProtocolError::Json)?;
    if let Some(content) = response.remove("content") {
        let blocks: Vec<Box<RawValue>> =
            serde_json::from_str(content.get()).map_err(|_| AnthropicProtocolError::Json)?;
        let mut restored = Vec::with_capacity(blocks.len());
        for block in blocks {
            restored.push(restore_raw_response_block(block, map)?);
        }
        response.insert(
            "content".to_owned(),
            raw_value(serde_json::to_string(&restored).map_err(|_| AnthropicProtocolError::Json)?)?,
        );
    }
    serde_json::to_string(&response).map_err(|_| AnthropicProtocolError::Json)
}

pub fn transform_messages_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), AnthropicProtocolError> {
    transform_messages_request_with_report(
        request,
        engine,
        policy,
        map,
        &mut RequestPrivacyReport::default(),
    )
}

pub fn transform_messages_request_with_report<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    if let Some(system) = request.get_mut("system") {
        transform_content(system, engine, policy, map, report)?;
    }
    if let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.get_mut("content") {
                transform_content(content, engine, policy, map, report)?;
            }
        }
    }
    enforce_content_coverage(policy, report)?;
    Ok(())
}

fn transform_content<P: Pseudonymizer>(
    content: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    match content {
        Value::String(text) => {
            *text = engine
                .transform_reported(text, policy, map, report)
                .map_err(|_| AnthropicProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(blocks) => {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => transform_field(block, "text", engine, policy, map, report)?,
                    Some("tool_result") => {
                        transform_tool_result(block, engine, policy, map, report)?
                    }
                    Some("tool_use") => transform_tool_input(block, engine, policy, map, report)?,
                    Some("thinking") | Some("redacted_thinking") => {}
                    Some(kind) => report.record_uninspected_content(anthropic_content_label(kind)),
                    None => report.record_uninspected_content("anthropic_content:unknown"),
                }
            }
        }
        Value::Null => {}
        _ => report.record_uninspected_content("anthropic_content:invalid"),
    }
    Ok(())
}

fn transform_raw_message<P: Pseudonymizer>(
    message: &RawValue,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<Box<RawValue>, AnthropicProtocolError> {
    let mut message: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(message.get()).map_err(|_| AnthropicProtocolError::Json)?;
    if let Some(content) = message.remove("content") {
        message.insert(
            "content".to_owned(),
            raw_value(transform_raw_content(
                content.get(),
                engine,
                policy,
                map,
                report,
            )?)?,
        );
    }
    raw_value(serde_json::to_string(&message).map_err(|_| AnthropicProtocolError::Json)?)
}

fn transform_raw_content<P: Pseudonymizer>(
    content: &str,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<String, AnthropicProtocolError> {
    let value: Value = serde_json::from_str(content).map_err(|_| AnthropicProtocolError::Json)?;
    match value {
        Value::String(text) => serde_json::to_string(
            engine
                .transform_reported(&text, policy, map, report)
                .map_err(|_| AnthropicProtocolError::Privacy)?
                .outbound_text(),
        )
        .map_err(|_| AnthropicProtocolError::Json),
        Value::Array(_) => {
            let blocks: Vec<Box<RawValue>> =
                serde_json::from_str(content).map_err(|_| AnthropicProtocolError::Json)?;
            let mut transformed = Vec::with_capacity(blocks.len());
            for block in blocks {
                transformed.push(transform_raw_block(block, engine, policy, map, report)?);
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
    report: &mut RequestPrivacyReport,
) -> Result<Box<RawValue>, AnthropicProtocolError> {
    let mut value: Value =
        serde_json::from_str(block.get()).map_err(|_| AnthropicProtocolError::Json)?;
    match value.get("type").and_then(Value::as_str) {
        Some("text") => transform_field(&mut value, "text", engine, policy, map, report)?,
        Some("tool_result") => transform_tool_result(&mut value, engine, policy, map, report)?,
        Some("tool_use") => transform_tool_input(&mut value, engine, policy, map, report)?,
        // Thinking, redacted_thinking, image and unknown blocks must remain raw. Anthropic
        // signatures are tied to the original serialized block rather than its JSON meaning.
        Some("thinking") | Some("redacted_thinking") => return Ok(block),
        Some(kind) => {
            report.record_uninspected_content(anthropic_content_label(kind));
            return Ok(block);
        }
        None => {
            report.record_uninspected_content("anthropic_content:unknown");
            return Ok(block);
        }
    }
    raw_value(serde_json::to_string(&value).map_err(|_| AnthropicProtocolError::Json)?)
}

fn restore_raw_response_block<B: MappingBackend>(
    block: Box<RawValue>,
    map: &PseudonymMap<B>,
) -> Result<Box<RawValue>, AnthropicProtocolError> {
    let mut value: Value =
        serde_json::from_str(block.get()).map_err(|_| AnthropicProtocolError::Json)?;
    match value.get("type").and_then(Value::as_str) {
        Some("text") => restore_field(&mut value, "text", map)?,
        Some("tool_use") => {
            if let Some(input) = value.get_mut("input") {
                restore_json_strings(input, map)?;
            }
        }
        _ => return Ok(block),
    }
    raw_value(serde_json::to_string(&value).map_err(|_| AnthropicProtocolError::Json)?)
}

fn transform_tool_result<P: Pseudonymizer>(
    block: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    let Some(content) = block.get_mut("content") else {
        return Ok(());
    };
    match content {
        Value::String(_) => transform_value_string(content, engine, policy, map, report),
        Value::Array(parts) => {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        transform_field(part, "text", engine, policy, map, report)?;
                    }
                    Some(kind) => {
                        report.record_uninspected_content(anthropic_tool_result_label(kind));
                    }
                    None => report.record_uninspected_content("anthropic_tool_result:unknown"),
                }
            }
            Ok(())
        }
        _ => {
            report.record_uninspected_content("anthropic_tool_result:invalid");
            Ok(())
        }
    }
}

fn anthropic_content_label(kind: &str) -> &'static str {
    match kind {
        "image" => "anthropic_content:image",
        "document" => "anthropic_content:document",
        "search_result" | "web_search_tool_result" => "anthropic_content:search_result",
        _ => "anthropic_content:unknown",
    }
}

fn anthropic_tool_result_label(kind: &str) -> &'static str {
    match kind {
        "image" => "anthropic_tool_result:image",
        "document" => "anthropic_tool_result:document",
        "search_result" => "anthropic_tool_result:search_result",
        _ => "anthropic_tool_result:unknown",
    }
}

fn transform_tool_input<P: Pseudonymizer>(
    block: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    if let Some(input) = block.get_mut("input") {
        transform_json_strings(input, engine, policy, map, report)?;
    }
    Ok(())
}

fn transform_json_strings<P: Pseudonymizer>(
    value: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    match value {
        Value::String(_) => transform_value_string(value, engine, policy, map, report),
        Value::Array(values) => {
            for value in values {
                transform_json_strings(value, engine, policy, map, report)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                transform_json_strings(value, engine, policy, map, report)?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn transform_value_string<P: Pseudonymizer>(
    value: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    *value = Value::String(
        engine
            .transform_reported(text, policy, map, report)
            .map_err(|_| AnthropicProtocolError::Privacy)?
            .outbound_text()
            .to_owned(),
    );
    Ok(())
}

fn restore_field<B: MappingBackend>(
    value: &mut Value,
    key: &str,
    map: &PseudonymMap<B>,
) -> Result<(), AnthropicProtocolError> {
    let Some(field) = value.get_mut(key) else {
        return Ok(());
    };
    restore_json_string(field, map)
}

fn restore_json_strings<B: MappingBackend>(
    value: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), AnthropicProtocolError> {
    match value {
        Value::String(_) => restore_json_string(value, map),
        Value::Array(values) => {
            for value in values {
                restore_json_strings(value, map)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                restore_json_strings(value, map)?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn restore_json_string<B: MappingBackend>(
    value: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), AnthropicProtocolError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    *value = Value::String(
        map.restore_text(text)
            .map_err(|_| AnthropicProtocolError::Privacy)?,
    );
    Ok(())
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
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    let Some(value) = block.get_mut(key) else {
        return Ok(());
    };
    transform_value_string(value, engine, policy, map, report)
}

fn enforce_content_coverage(
    policy: &Policy,
    report: &mut RequestPrivacyReport,
) -> Result<(), AnthropicProtocolError> {
    if policy.blocks_uninspected_content() && report.has_uninspected_content() {
        report.record_policy_action(PolicyAction::Block);
        return Err(AnthropicProtocolError::Privacy);
    }
    Ok(())
}
