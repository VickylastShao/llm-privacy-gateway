use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenAiProtocolError {
    #[error("privacy transformation failed")]
    Privacy,
}
