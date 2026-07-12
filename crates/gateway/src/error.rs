use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum GatewayError {
    #[error("gateway must listen on a loopback address")]
    NonLoopbackBind,
    #[error("local API key is missing or unknown")]
    Unauthorized,
    #[error("local API key is not valid for this protocol")]
    ProtocolMismatch,
    #[error("gateway listener failed")]
    Listener,
    #[error("request body is invalid JSON")]
    InvalidJson,
    #[error("privacy transformation blocked the request")]
    Blocked,
    #[error("upstream request failed")]
    Upstream,
}
