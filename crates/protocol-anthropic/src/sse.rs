use serde_json::Value;
use session_map::{MappingBackend, PseudonymMap};
use thiserror::Error;

const PREFIX: &str = "__LPG_";

#[derive(Debug, Default)]
pub struct AnthropicSseTransformer {
    raw: String,
    utf8_tail: Vec<u8>,
    pending: String,
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
        while let Some(end) = self.raw.find("\n\n") {
            let event = self.raw[..end].to_owned();
            self.raw = self.raw[end + 2..].to_owned();
            output.push_str(&transform_raw_event(&event, &mut self.pending, map)?);
        }
        Ok(output.into_bytes())
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, AnthropicSseError> {
        if !self.utf8_tail.is_empty()
            || !self.raw.is_empty()
            || !std::mem::take(&mut self.pending).is_empty()
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
    pending: &mut String,
    map: &PseudonymMap<B>,
) -> Result<String, AnthropicSseError> {
    let event = raw_event
        .lines()
        .find_map(|line| line.strip_prefix("event: "));
    let data = raw_event
        .lines()
        .find_map(|line| line.strip_prefix("data: "));
    let Some(data) = data else {
        return Ok(format!("{raw_event}\n\n"));
    };
    if event != Some("content_block_delta") {
        return Ok(format!("{raw_event}\n\n"));
    }
    let mut value: Value = serde_json::from_str(data).map_err(|_| AnthropicSseError::Invalid)?;
    let Some(text_value) = value
        .get_mut("delta")
        .and_then(|delta| delta.get_mut("text"))
    else {
        return Ok(format!("{raw_event}\n\n"));
    };
    let Some(text) = text_value.as_str() else {
        return Err(AnthropicSseError::Invalid);
    };
    pending.push_str(text);
    let (restored, remainder) = restore_complete(pending, map)?;
    *pending = remainder;
    *text_value = Value::String(restored);
    Ok(format!(
        "event: content_block_delta\ndata: {}\n\n",
        serde_json::to_string(&value).map_err(|_| AnthropicSseError::Invalid)?
    ))
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
