use serde_json::Value;
use session_map::{MappingBackend, PseudonymMap};
use thiserror::Error;

const PREFIX: &str = "__LPG_";

#[derive(Debug, Error)]
pub enum AnthropicSseError {
    #[error("invalid SSE event")]
    Invalid,
    #[error("mapping restoration failed")]
    Mapping,
    #[error("incomplete placeholder")]
    Incomplete,
}

pub fn restore_anthropic_sse<B: MappingBackend>(
    input: &str,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicSseError> {
    let mut output = String::new();
    let mut pending = String::new();
    for raw_event in input.split("\n\n") {
        if raw_event.is_empty() {
            continue;
        }
        let event = raw_event
            .lines()
            .find_map(|line| line.strip_prefix("event: "));
        let data = raw_event
            .lines()
            .find_map(|line| line.strip_prefix("data: "));
        let Some(data) = data else {
            output.push_str(raw_event);
            output.push_str("\n\n");
            continue;
        };
        if event != Some("content_block_delta") {
            output.push_str(raw_event);
            output.push_str("\n\n");
            continue;
        }
        let mut value: Value =
            serde_json::from_str(data).map_err(|_| AnthropicSseError::Invalid)?;
        let Some(text_value) = value
            .get_mut("delta")
            .and_then(|delta| delta.get_mut("text"))
        else {
            output.push_str(raw_event);
            output.push_str("\n\n");
            continue;
        };
        let Some(text) = text_value.as_str() else {
            return Err(AnthropicSseError::Invalid);
        };
        pending.push_str(text);
        let (restored, remainder) = restore_complete(&pending, map)?;
        pending = remainder;
        *text_value = Value::String(restored);
        output.push_str("event: content_block_delta\ndata: ");
        output.push_str(&serde_json::to_string(&value).map_err(|_| AnthropicSseError::Invalid)?);
        output.push_str("\n\n");
    }
    if !pending.is_empty() {
        return Err(AnthropicSseError::Incomplete);
    }
    Ok(output)
}

fn restore_complete<B: MappingBackend>(
    input: &str,
    map: &PseudonymMap<B>,
) -> Result<(String, String), AnthropicSseError> {
    let mut remaining = input.to_owned();
    let mut output = String::new();
    loop {
        let Some(start) = remaining.find(PREFIX) else {
            let keep = (1..PREFIX.len())
                .rev()
                .find(|&count| remaining.ends_with(&PREFIX[..count]))
                .unwrap_or(0);
            let split = remaining.len() - keep;
            output.push_str(&remaining[..split]);
            return Ok((output, remaining[split..].to_owned()));
        };
        output.push_str(&remaining[..start]);
        let after = start + PREFIX.len();
        let Some(relative_end) = remaining[after..].find("__") else {
            return Ok((output, remaining[start..].to_owned()));
        };
        let end = after + relative_end + 2;
        let token = &remaining[start..end];
        output.push_str(
            &map.restore_text(token)
                .map_err(|_| AnthropicSseError::Mapping)?,
        );
        remaining = remaining[end..].to_owned();
    }
}
