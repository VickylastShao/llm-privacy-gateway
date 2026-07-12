use crate::{Detection, PrivacyError};

pub trait Detector: Send + Sync {
    fn name(&self) -> &'static str;
    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError>;
}
