const MAX_EVENT_BYTES: usize = 1024 * 1024;

use crate::OpenAiProtocolError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
    event: Option<String>,
    data: Vec<String>,
}

impl SseDecoder {
    pub fn push(&mut self, chunk: &str) -> Result<Vec<SseEvent>, OpenAiProtocolError> {
        self.buffer.push_str(chunk);
        if self.buffer.len() > MAX_EVENT_BYTES {
            return Err(OpenAiProtocolError::SseEventTooLarge);
        }
        let mut output = Vec::new();
        while let Some(newline) = self.buffer.find('\n') {
            let mut line = self.buffer[..newline].to_owned();
            self.buffer = self.buffer[newline + 1..].to_owned();
            if line.ends_with('\r') {
                line.pop();
            }
            if line.is_empty() {
                if !self.data.is_empty() || self.event.is_some() {
                    output.push(SseEvent {
                        event: self.event.take(),
                        data: self.data.join("\n"),
                    });
                    self.data.clear();
                }
            } else if let Some(value) = line.strip_prefix("event:") {
                self.event = Some(value.trim_start().to_owned());
            } else if let Some(value) = line.strip_prefix("data:") {
                self.data
                    .push(value.strip_prefix(' ').unwrap_or(value).to_owned());
            }
        }
        Ok(output)
    }
}
