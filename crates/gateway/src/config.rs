use std::net::SocketAddr;

use crate::GatewayError;

#[derive(Debug, Clone, Copy)]
pub struct GatewayConfig {
    bind: SocketAddr,
}

impl GatewayConfig {
    pub fn new(bind: SocketAddr) -> Result<Self, GatewayError> {
        if !bind.ip().is_loopback() {
            return Err(GatewayError::NonLoopbackBind);
        }
        Ok(Self { bind })
    }
    pub const fn bind(&self) -> SocketAddr {
        self.bind
    }
}
