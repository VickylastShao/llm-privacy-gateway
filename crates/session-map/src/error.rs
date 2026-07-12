use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapError {
    #[error("mapping backend operation failed")]
    Backend,
    #[error("encrypted mapping authentication failed")]
    AuthenticationFailed,
    #[error("mapping record is malformed")]
    InvalidRecord,
    #[error("cryptographic operation failed")]
    Crypto,
}
