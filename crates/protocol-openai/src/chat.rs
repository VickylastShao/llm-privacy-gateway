use privacy_core::{Policy, PolicyAction, PrivacyEngine, RequestPrivacyReport};
use serde_json::Value;
use session_map::Pseudonymizer;

use crate::OpenAiProtocolError;

pub fn transform_chat_request<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
) -> Result<(), OpenAiProtocolError> {
    transform_chat_request_with_report(
        request,
        engine,
        policy,
        map,
        &mut RequestPrivacyReport::default(),
    )
}

pub fn transform_chat_request_with_report<P: Pseudonymizer>(
    request: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    let Some(messages) = request.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for message in messages {
        transform_message(message, engine, policy, map, report)?;
    }
    enforce_content_coverage(policy, report)?;
    Ok(())
}

fn transform_message<P: Pseudonymizer>(
    message: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    if let Some(content) = message.get_mut("content") {
        transform_content(content, engine, policy, map, report)?;
    }
    if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
        for call in calls {
            if let Some(arguments_value) = call
                .get_mut("function")
                .and_then(|function| function.get_mut("arguments"))
            {
                transform_json_strings(arguments_value, engine, policy, map, report)?;
            }
        }
    }
    Ok(())
}

fn transform_json_strings<P: Pseudonymizer>(
    value: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    match value {
        Value::String(text) => {
            *text = engine
                .transform_reported(text, policy, map, report)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
            Ok(())
        }
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
                match part.get("type").and_then(Value::as_str) {
                    Some("text") | Some("input_text") => {
                        let Some(text_value) = part.get_mut("text") else {
                            continue;
                        };
                        let Some(text) = text_value.as_str() else {
                            report.record_uninspected_content("openai_non_string_text");
                            continue;
                        };
                        let transformed = engine
                            .transform_reported(text, policy, map, report)
                            .map_err(|_| OpenAiProtocolError::Privacy)?;
                        *text_value = Value::String(transformed.outbound_text().to_owned());
                    }
                    Some(kind) => report.record_uninspected_content(chat_content_label(kind)),
                    None => report.record_uninspected_content("openai_chat_content:unknown"),
                }
            }
        }
        Value::Null => {}
        _ => report.record_uninspected_content("openai_chat_content:invalid"),
    }
    Ok(())
}

fn chat_content_label(kind: &str) -> &'static str {
    match kind {
        "image_url" | "input_image" => "openai_chat_content:image",
        "input_audio" | "audio" => "openai_chat_content:audio",
        "file" | "input_file" => "openai_chat_content:file",
        _ => "openai_chat_content:unknown",
    }
}

fn enforce_content_coverage(
    policy: &Policy,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    if policy.blocks_uninspected_content() && report.has_uninspected_content() {
        report.record_policy_action(PolicyAction::Block);
        return Err(OpenAiProtocolError::Privacy);
    }
    Ok(())
}
