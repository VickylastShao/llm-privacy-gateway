#![forbid(unsafe_code)]

use std::{
    collections::{HashSet, VecDeque},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use activity_store::{Activity, Repository, StoreError, UsageRecord};
use gateway::{ActivityEvent, ActivitySink, UsageEvent, UsageSink};
use serde::Serialize;

const RETENTION_HOURS: u64 = 168;
const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1_000;
const SESSION_LIMIT: usize = 10_000;
const QUEUE_CAPACITY: usize = 1_024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityPreview {
    pub kind: String,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityView {
    pub id: String,
    pub occurred_at_ms: i64,
    pub profile_id: String,
    pub endpoint: String,
    pub model: String,
    pub status: u16,
    pub latency_ms: u64,
    pub category_counts: std::collections::BTreeMap<String, u64>,
    pub actions: Vec<String>,
    pub request_id: String,
    pub previews: Vec<ActivityPreview>,
    pub preview_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActivityQuery {
    pub activities: Vec<ActivityView>,
    pub total_activity_count: u64,
    pub session_preview_count: u64,
    pub persist_dropped_count: u64,
    pub retention_hours: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageView {
    pub profile_id: String,
    pub request_count: u64,
    pub metered_request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsageQuery {
    pub profiles: Vec<UsageView>,
    pub retention_hours: u64,
}

pub struct ActivityService {
    repository: Arc<Mutex<Repository>>,
    session: Mutex<VecDeque<ActivityEvent>>,
    sender: SyncSender<ActivityEvent>,
    persist_dropped: Arc<AtomicU64>,
}

impl ActivityService {
    pub fn open(path: impl AsRef<Path>) -> Result<Arc<Self>, StoreError> {
        let repository = Arc::new(Mutex::new(Repository::open(path)?));
        {
            let repository = repository.lock().map_err(|_| StoreError::Database)?;
            repository.prune_activities(now_ms())?;
        }
        let (sender, receiver) = sync_channel::<ActivityEvent>(QUEUE_CAPACITY);
        let persist_dropped = Arc::new(AtomicU64::new(0));
        let writer_repository = Arc::clone(&repository);
        let writer_dropped = Arc::clone(&persist_dropped);
        std::thread::Builder::new()
            .name("lpg-activity-writer".to_owned())
            .spawn(move || {
                while let Ok(event) = receiver.recv() {
                    let activity = stored_activity(&event);
                    let persisted = writer_repository
                        .lock()
                        .ok()
                        .and_then(|repository| repository.record_activity(&activity).ok())
                        .is_some();
                    if !persisted {
                        writer_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .map_err(|_| StoreError::Database)?;
        Ok(Arc::new(Self {
            repository,
            session: Mutex::new(VecDeque::new()),
            sender,
            persist_dropped,
        }))
    }

    pub fn query(&self, limit: usize, now_ms: i64) -> Result<ActivityQuery, StoreError> {
        let limit = limit.min(100);
        let cutoff = now_ms.saturating_sub(RETENTION_MS);
        let mut session = self.session.lock().map_err(|_| StoreError::Database)?;
        session.retain(|event| event.occurred_at_ms >= cutoff);
        let repository = self.repository.lock().map_err(|_| StoreError::Database)?;
        repository.prune_activities(now_ms)?;
        let persisted = repository.activities(limit)?;
        let mut ids = repository
            .activity_ids()?
            .into_iter()
            .collect::<HashSet<_>>();
        ids.extend(session.iter().map(|event| event.id));

        let mut views = session.iter().rev().map(session_view).collect::<Vec<_>>();
        let session_ids = session.iter().map(|event| event.id).collect::<HashSet<_>>();
        views.extend(
            persisted
                .iter()
                .filter(|activity| !session_ids.contains(&activity.id))
                .map(persisted_view),
        );
        views.sort_by(|left, right| {
            right
                .occurred_at_ms
                .cmp(&left.occurred_at_ms)
                .then_with(|| right.id.cmp(&left.id))
        });
        views.truncate(limit);

        Ok(ActivityQuery {
            activities: views,
            total_activity_count: u64::try_from(ids.len()).unwrap_or(u64::MAX),
            session_preview_count: u64::try_from(
                session
                    .iter()
                    .filter(|event| !event.report.previews().is_empty())
                    .count(),
            )
            .unwrap_or(u64::MAX),
            persist_dropped_count: self.persist_dropped.load(Ordering::Relaxed),
            retention_hours: RETENTION_HOURS,
        })
    }

    pub fn persisted_count(&self) -> Result<u64, StoreError> {
        self.repository
            .lock()
            .map_err(|_| StoreError::Database)?
            .activity_count()
    }

    pub fn query_usage(&self, now_ms: i64) -> Result<UsageQuery, StoreError> {
        let repository = self.repository.lock().map_err(|_| StoreError::Database)?;
        repository.prune_activities(now_ms)?;
        let profiles = repository
            .usage_totals(now_ms.saturating_sub(RETENTION_MS))?
            .into_iter()
            .map(|usage| UsageView {
                profile_id: usage.profile_id.to_string(),
                request_count: usage.request_count,
                metered_request_count: usage.metered_request_count,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
                cached_tokens: usage.cached_tokens,
            })
            .collect();
        Ok(UsageQuery {
            profiles,
            retention_hours: RETENTION_HOURS,
        })
    }
}

impl ActivitySink for ActivityService {
    fn publish(&self, event: ActivityEvent) -> bool {
        let mut session = match self.session.lock() {
            Ok(session) => session,
            Err(_) => {
                self.persist_dropped.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        };
        session.push_back(event.clone());
        if session.len() > SESSION_LIMIT {
            session.pop_front();
        }
        drop(session);
        match self.sender.try_send(event) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.persist_dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

impl UsageSink for ActivityService {
    fn publish_usage(&self, event: UsageEvent) -> bool {
        self.repository
            .lock()
            .ok()
            .and_then(|repository| {
                repository
                    .record_usage(&UsageRecord {
                        id: event.id,
                        occurred_at_ms: event.occurred_at_ms,
                        profile_id: event.profile_id,
                        model: event.model,
                        reported: event.reported,
                        input_tokens: event.usage.input_tokens,
                        output_tokens: event.usage.output_tokens,
                        total_tokens: event.usage.total_tokens,
                        cached_tokens: event.usage.cached_tokens,
                    })
                    .ok()
            })
            .is_some()
    }
}

fn stored_activity(event: &ActivityEvent) -> Activity {
    Activity {
        id: event.id,
        occurred_at_ms: event.occurred_at_ms,
        profile_id: event.profile_id,
        endpoint: event.endpoint.clone(),
        model: event.model.clone(),
        status: event.status,
        latency_ms: event.latency_ms,
        category_counts: event.report.category_counts().clone(),
        actions: event.report.actions().to_vec(),
        request_id: event.request_id.clone(),
    }
}

fn session_view(event: &ActivityEvent) -> ActivityView {
    ActivityView {
        id: event.id.to_string(),
        occurred_at_ms: event.occurred_at_ms,
        profile_id: event.profile_id.to_string(),
        endpoint: event.endpoint.clone(),
        model: event.model.clone(),
        status: event.status,
        latency_ms: event.latency_ms,
        category_counts: event.report.category_counts().clone(),
        actions: event.report.actions().to_vec(),
        request_id: event.request_id.clone(),
        previews: event
            .report
            .previews()
            .iter()
            .map(|preview| ActivityPreview {
                kind: preview.kind().to_owned(),
                display: preview.display().to_owned(),
            })
            .collect(),
        preview_available: true,
    }
}

fn persisted_view(activity: &Activity) -> ActivityView {
    ActivityView {
        id: activity.id.to_string(),
        occurred_at_ms: activity.occurred_at_ms,
        profile_id: activity.profile_id.to_string(),
        endpoint: activity.endpoint.clone(),
        model: activity.model.clone(),
        status: activity.status,
        latency_ms: activity.latency_ms,
        category_counts: activity.category_counts.clone(),
        actions: activity.actions.clone(),
        request_id: activity.request_id.clone(),
        previews: Vec::new(),
        preview_available: false,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
        .unwrap_or(0)
}
