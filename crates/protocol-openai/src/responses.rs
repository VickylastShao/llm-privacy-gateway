use privacy_core::{Policy, PrivacyEngine, RequestPrivacyReport};
use serde_json::Value;
use session_map::Pseudonymizer;

use crate::OpenAiProtocolError;

pub fn transform_responses_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    transform_responses_request_with_report(
        request,
        engine,
        policy,
        map,
        &mut RequestPrivacyReport::default(),
    )
}

pub fn transform_responses_request_with_report<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    transform_string_field(request, "instructions", engine, policy, map, report)?;
    if let Some(input) = request.get_mut("input") {
        transform_input(input, engine, policy, map, report)?;
    }
    Ok(())
}

fn transform_input<P: Pseudonymizer>(
    input: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    match input {
        Value::String(text) => {
            *text = engine
                .transform_reported(text, policy, map, report)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(items) => {
            for item in items {
                if let Some(content) = item.get_mut("content") {
                    transform_content(content, engine, policy, map, report)?;
                }
                if matches!(
                    item.get("type").and_then(Value::as_str),
                    Some("function_call_output")
                ) {
                    transform_string_field(item, "output", engine, policy, map, report)?;
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
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    match content {
        Value::String(text) => {
            *text = engine
                .transform_reported(text, policy, map, report)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
        }
        Value::Array(parts) => {
            for part in parts {
                if matches!(part.get("type").and_then(Value::as_str), Some("input_text")) {
                    transform_string_field(part, "text", engine, policy, map, report)?;
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
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    let Some(text_value) = object.get_mut(key) else {
        return Ok(());
    };
    let Some(text) = text_value.as_str() else {
        return Ok(());
    };
    let transformed = engine
        .transform_reported(text, policy, map, report)
        .map_err(|_| OpenAiProtocolError::Privacy)?;
    *text_value = Value::String(transformed.outbound_text().to_owned());
    Ok(())
}
