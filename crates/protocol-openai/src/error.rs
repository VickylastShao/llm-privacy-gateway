use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenAiProtocolError {
    #[error("privacy transformation failed")]
    Privacy,
    #[error("SSE event exceeds the configured limit")]
    SseEventTooLarge,
    #[error("SSE data payload is invalid JSON")]
    InvalidSseJson,
    #[error("SSE stream ended with an incomplete placeholder")]
    IncompletePlaceholder,
    #[error("SSE stream ended with an incomplete event")]
    IncompleteSseEvent,
}
