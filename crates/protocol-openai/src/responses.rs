use privacy_core::{Policy, PolicyAction, PrivacyEngine, RequestPrivacyReport};
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
    enforce_content_coverage(policy, report)?;
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
                match item.get("type").and_then(Value::as_str) {
                    Some("message") => {
                        if let Some(content) = item.get_mut("content") {
                            transform_content(content, engine, policy, map, report)?;
                        }
                    }
                    Some("function_call_output") => {
                        if let Some(output) = item.get_mut("output") {
                            transform_function_output(output, engine, policy, map, report)?;
                        }
                    }
                    Some("function_call") => {
                        if item.get("arguments").and_then(Value::as_str).is_some() {
                            transform_string_field(item, "arguments", engine, policy, map, report)?;
                        } else {
                            report.record_uninspected_content(
                                "openai_function_call:invalid_arguments",
                            );
                        }
                    }
                    Some("item_reference") | Some("reasoning") => {}
                    Some(kind) => report.record_uninspected_content(responses_item_label(kind)),
                    None => report.record_uninspected_content("openai_responses_item:unknown"),
                }
            }
        }
        _ => report.record_uninspected_content("openai_responses_input:invalid"),
    }
    Ok(())
}

fn transform_function_output<P: Pseudonymizer>(
    output: &mut Value,
    engine: &PrivacyEngine,
    policy: &Policy,
    map: &mut P,
    report: &mut RequestPrivacyReport,
) -> Result<(), OpenAiProtocolError> {
    match output {
        Value::String(text) => {
            *text = engine
                .transform_reported(text, policy, map, report)
                .map_err(|_| OpenAiProtocolError::Privacy)?
                .outbound_text()
                .to_owned();
            Ok(())
        }
        Value::Array(parts) => {
            for part in parts {
                match part.get("type").and_then(Value::as_str) {
                    Some("input_text") => {
                        transform_string_field(part, "text", engine, policy, map, report)?;
                    }
                    Some(kind) => report.record_uninspected_content(responses_output_label(kind)),
                    None => report.record_uninspected_content("openai_function_output:unknown"),
                }
            }
            Ok(())
        }
        _ => {
            report.record_uninspected_content("openai_function_output:invalid");
            Ok(())
        }
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
                    Some("input_text") => {
                        transform_string_field(part, "text", engine, policy, map, report)?;
                    }
                    Some(kind) => report.record_uninspected_content(responses_content_label(kind)),
                    None => report.record_uninspected_content("openai_responses_content:unknown"),
                }
            }
        }
        Value::Null => {}
        _ => report.record_uninspected_content("openai_responses_content:invalid"),
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

fn responses_item_label(kind: &str) -> &'static str {
    match kind {
        "computer_call_output" => "openai_responses_item:computer_output",
        "custom_tool_call_output" => "openai_responses_item:custom_tool_output",
        "local_shell_call_output" => "openai_responses_item:shell_output",
        _ => "openai_responses_item:unknown",
    }
}

fn responses_output_label(kind: &str) -> &'static str {
    match kind {
        "input_image" => "openai_function_output:image",
        "input_file" => "openai_function_output:file",
        "input_audio" => "openai_function_output:audio",
        _ => "openai_function_output:unknown",
    }
}

fn responses_content_label(kind: &str) -> &'static str {
    match kind {
        "input_image" => "openai_responses_content:image",
        "input_file" => "openai_responses_content:file",
        "input_audio" => "openai_responses_content:audio",
        _ => "openai_responses_content:unknown",
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
