use serde_json::Value;
use session_map::{MappingBackend, PseudonymMap};
use thiserror::Error;

const PREFIX: &str = "__LPG_";

#[derive(Debug, Default)]
pub struct AnthropicSseTransformer {
    raw: String,
    utf8_tail: Vec<u8>,
    pending: String,
    input_json: Option<BufferedInputJson>,
}

#[derive(Debug)]
struct BufferedInputJson {
    index: Option<Value>,
    template: Value,
    partial_json: String,
    delimiter: String,
    line_ending: String,
}

impl AnthropicSseTransformer {
    pub fn push_bytes<B: MappingBackend>(
        &mut self,
        bytes: &[u8],
        map: &PseudonymMap<B>,
    ) -> Result<Vec<u8>, AnthropicSseError> {
        self.utf8_tail.extend_from_slice(bytes);
        let text = match std::str::from_utf8(&self.utf8_tail) {
            Ok(text) => {
                let value = text.to_owned();
                self.utf8_tail.clear();
                value
            }
            Err(error) if error.error_len().is_none() => {
                let value = std::str::from_utf8(&self.utf8_tail[..error.valid_up_to()])
                    .map_err(|_| AnthropicSseError::Invalid)?
                    .to_owned();
                self.utf8_tail = self.utf8_tail[error.valid_up_to()..].to_vec();
                value
            }
            Err(_) => return Err(AnthropicSseError::Invalid),
        };
        self.raw.push_str(&text);
        let mut output = String::new();
        while let Some((end, delimiter_len)) = find_event_boundary(&self.raw) {
            let event = self.raw[..end].to_owned();
            let delimiter = self.raw[end..end + delimiter_len].to_owned();
            self.raw = self.raw[end + delimiter_len..].to_owned();
            output.push_str(&transform_raw_event(
                &event,
                &delimiter,
                &mut self.pending,
                &mut self.input_json,
                map,
            )?);
        }
        Ok(output.into_bytes())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, AnthropicSseError> {
        if !self.utf8_tail.is_empty()
            || !self.raw.is_empty()
            || !std::mem::take(&mut self.pending).is_empty()
            || self.input_json.is_some()
        {
            return Err(AnthropicSseError::Incomplete);
        }
        Ok(Vec::new())
    }
}

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
    let mut transformer = AnthropicSseTransformer::default();
    let mut output = transformer.push_bytes(input.as_bytes(), map)?;
    output.extend(transformer.finish()?);
    String::from_utf8(output).map_err(|_| AnthropicSseError::Invalid)
}

fn transform_raw_event<B: MappingBackend>(
    raw_event: &str,
    delimiter: &str,
    pending: &mut String,
    input_json: &mut Option<BufferedInputJson>,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicSseError> {
    let lines = raw_event.lines().collect::<Vec<_>>();
    let event = lines
        .iter()
        .find_map(|line| line.strip_prefix("event:").map(str::trim_start));
    if event == Some("content_block_start") {
        return restore_content_block_start(raw_event, delimiter, map);
    }
    if event == Some("content_block_stop") {
        return flush_input_json(raw_event, delimiter, input_json, map);
    }
    if event != Some("content_block_delta") {
        return Ok(format!("{raw_event}{delimiter}"));
    }
    let data = lines
        .iter()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return Err(AnthropicSseError::Invalid);
    }
    let data = data.join("\n");
    let mut value: Value = serde_json::from_str(&data).map_err(|_| AnthropicSseError::Invalid)?;
    if value
        .get("delta")
        .and_then(|delta| delta.get("type"))
        .and_then(Value::as_str)
        == Some("input_json_delta")
    {
        let Some(partial_json) = value
            .get("delta")
            .and_then(|delta| delta.get("partial_json"))
            .and_then(Value::as_str)
        else {
            return Err(AnthropicSseError::Invalid);
        };
        let partial_json = partial_json.to_owned();
        let index = value.get("index").cloned();
        if let Some(buffered) = input_json.as_mut() {
            if buffered.index != index {
                return Err(AnthropicSseError::Invalid);
            }
            buffered.partial_json.push_str(&partial_json);
        } else {
            *input_json = Some(BufferedInputJson {
                index,
                template: value,
                partial_json,
                delimiter: delimiter.to_owned(),
                line_ending: if raw_event.contains("\r\n") {
                    "\r\n".to_owned()
                } else {
                    "\n".to_owned()
                },
            });
        }
        return Ok(String::new());
    }
    let Some(text_value) = value
        .get_mut("delta")
        .and_then(|delta| delta.get_mut("text"))
    else {
        return Ok(format!("{raw_event}{delimiter}"));
    };
    let Some(text) = text_value.as_str() else {
        return Err(AnthropicSseError::Invalid);
    };
    pending.push_str(text);
    let (restored, remainder) = restore_complete(pending, map)?;
    *pending = remainder;
    *text_value = Value::String(restored);
    let line_ending = if raw_event.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let payload = serde_json::to_string(&value).map_err(|_| AnthropicSseError::Invalid)?;
    let mut output = Vec::new();
    let mut replaced_data = false;
    for line in lines {
        if line.strip_prefix("data:").is_some() {
            if replaced_data {
                continue;
            }
            output.push(format!("data: {payload}"));
            replaced_data = true;
        } else {
            output.push(line.to_owned());
        }
    }
    Ok(format!("{}{delimiter}", output.join(line_ending)))
}

fn restore_content_block_start<B: MappingBackend>(
    raw_event: &str,
    delimiter: &str,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicSseError> {
    let data = raw_event
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return Err(AnthropicSseError::Invalid);
    }
    let mut value: Value =
        serde_json::from_str(&data.join("\n")).map_err(|_| AnthropicSseError::Invalid)?;
    let Some(block) = value.get_mut("content_block") else {
        return Ok(format!("{raw_event}{delimiter}"));
    };
    match block.get("type").and_then(Value::as_str) {
        Some("text") => {
            if let Some(text) = block.get_mut("text") {
                restore_json_string(text, map)?;
            }
        }
        Some("tool_use") => {
            if let Some(input) = block.get_mut("input") {
                restore_json_strings(input, map)?;
            }
        }
        _ => return Ok(format!("{raw_event}{delimiter}")),
    }
    replace_event_data(raw_event, delimiter, &value)
}

fn restore_json_strings<B: MappingBackend>(
    value: &mut Value,
    map: &PseudonymMap<B>,
) -> Result<(), AnthropicSseError> {
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
) -> Result<(), AnthropicSseError> {
    let Some(text) = value.as_str() else {
        return Ok(());
    };
    *value = Value::String(
        map.restore_text(text)
            .map_err(|_| AnthropicSseError::Mapping)?,
    );
    Ok(())
}

fn replace_event_data(
    raw_event: &str,
    delimiter: &str,
    value: &Value,
) -> Result<String, AnthropicSseError> {
    let payload = serde_json::to_string(value).map_err(|_| AnthropicSseError::Invalid)?;
    let line_ending = if raw_event.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = Vec::new();
    let mut replaced_data = false;
    for line in raw_event.lines() {
        if line.strip_prefix("data:").is_some() {
            if replaced_data {
                continue;
            }
            output.push(format!("data: {payload}"));
            replaced_data = true;
        } else {
            output.push(line.to_owned());
        }
    }
    Ok(format!("{}{delimiter}", output.join(line_ending)))
}

fn flush_input_json<B: MappingBackend>(
    raw_event: &str,
    delimiter: &str,
    input_json: &mut Option<BufferedInputJson>,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicSseError> {
    let Some(buffered) = input_json.as_ref() else {
        return Ok(format!("{raw_event}{delimiter}"));
    };
    let data = raw_event
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
        .collect::<Vec<_>>();
    if data.is_empty() {
        return Err(AnthropicSseError::Invalid);
    }
    let stop: Value =
        serde_json::from_str(&data.join("\n")).map_err(|_| AnthropicSseError::Invalid)?;
    if stop.get("index").cloned() != buffered.index {
        return Ok(format!("{raw_event}{delimiter}"));
    }
    let mut buffered = input_json.take().ok_or(AnthropicSseError::Invalid)?;
    let restored = map
        .restore_text(&buffered.partial_json)
        .map_err(|_| AnthropicSseError::Mapping)?;
    serde_json::from_str::<Value>(&restored).map_err(|_| AnthropicSseError::Invalid)?;
    *buffered
        .template
        .get_mut("delta")
        .and_then(|delta| delta.get_mut("partial_json"))
        .ok_or(AnthropicSseError::Invalid)? = Value::String(restored);
    let payload =
        serde_json::to_string(&buffered.template).map_err(|_| AnthropicSseError::Invalid)?;
    let delta_event = format!(
        "event: content_block_delta{}data: {payload}{}",
        buffered.line_ending, buffered.delimiter
    );
    Ok(format!("{delta_event}{raw_event}{delimiter}"))
}

fn find_event_boundary(input: &str) -> Option<(usize, usize)> {
    let lf = input.find("\n\n").map(|index| (index, 2));
    let crlf = input.find("\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 < right.0 { left } else { right }),
        (Some(boundary), None) | (None, Some(boundary)) => Some(boundary),
        (None, None) => None,
    }
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
