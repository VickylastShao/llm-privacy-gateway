use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
}

impl TokenUsage {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.input_tokens == 0
            && self.output_tokens == 0
            && self.total_tokens == 0
            && self.cached_tokens == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    pub id: Uuid,
    pub occurred_at_ms: i64,
    pub profile_id: Uuid,
    pub model: String,
    pub reported: bool,
    pub usage: TokenUsage,
}

pub trait UsageSink: Send + Sync {
    fn publish_usage(&self, event: UsageEvent) -> bool;
}

#[derive(Debug, Default)]
pub struct NoopUsageSink;

impl UsageSink for NoopUsageSink {
    fn publish_usage(&self, _event: UsageEvent) -> bool {
        true
    }
}
