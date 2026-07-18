use privacy_core::RequestPrivacyReport;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub occurred_at_ms: i64,
    pub profile_id: Uuid,
    pub endpoint: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u64,
    pub request_id: String,
    pub report: RequestPrivacyReport,
}

pub trait ActivitySink: Send + Sync {
    fn publish(&self, event: ActivityEvent) -> bool;
}

#[derive(Debug, Default)]
pub struct NoopActivitySink;

impl ActivitySink for NoopActivitySink {
    fn publish(&self, _event: ActivityEvent) -> bool {
        true
    }
}
