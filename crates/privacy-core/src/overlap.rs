use crate::{Detection, EntityKind};

pub(crate) fn resolve(mut detections: Vec<Detection>) -> Vec<Detection> {
    detections.sort_by(|left, right| {
        priority(right.kind())
            .cmp(&priority(left.kind()))
            .then_with(|| right.confidence().cmp(&left.confidence()))
            .then_with(|| (right.end() - right.start()).cmp(&(left.end() - left.start())))
            .then_with(|| left.start().cmp(&right.start()))
    });
    let mut kept = Vec::new();
    for detection in detections {
        let overlaps = kept.iter().any(|existing: &Detection| {
            detection.start() < existing.end() && existing.start() < detection.end()
        });
        if !overlaps {
            kept.push(detection);
        }
    }
    kept.sort_by_key(Detection::start);
    kept
}

fn priority(kind: &EntityKind) -> u8 {
    match kind {
        EntityKind::PrivateKey => 100,
        EntityKind::ApiKey => 95,
        EntityKind::Password => 90,
        EntityKind::ConnectionString => 85,
        EntityKind::Jwt => 80,
        EntityKind::ChinaResidentId
        | EntityKind::UnifiedSocialCreditCode
        | EntityKind::BankCard => 70,
        EntityKind::Custom(_) => 60,
        EntityKind::Phone | EntityKind::Email => 50,
        EntityKind::Ipv4
        | EntityKind::Ipv6
        | EntityKind::MacAddress
        | EntityKind::InternalHostname
        | EntityKind::Coordinate => 40,
    }
}
