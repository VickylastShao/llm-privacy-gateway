use std::collections::BTreeMap;

use serde::Serialize;

use crate::{Decision, EntityKind, PolicyAction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrivacyPreview {
    kind: String,
    display: String,
}

impl PrivacyPreview {
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn display(&self) -> &str {
        &self.display
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RequestPrivacyReport {
    category_counts: BTreeMap<String, u64>,
    actions: Vec<String>,
    previews: Vec<PrivacyPreview>,
}

impl RequestPrivacyReport {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.category_counts.is_empty()
    }

    #[must_use]
    pub const fn category_counts(&self) -> &BTreeMap<String, u64> {
        &self.category_counts
    }

    #[must_use]
    pub fn actions(&self) -> &[String] {
        &self.actions
    }

    #[must_use]
    pub fn previews(&self) -> &[PrivacyPreview] {
        &self.previews
    }

    pub(crate) fn record(&mut self, decision: &Decision, value: &str) {
        if decision.action() == PolicyAction::Allow {
            return;
        }
        let kind = entity_label(decision.detection().kind()).to_owned();
        *self.category_counts.entry(kind.clone()).or_default() += 1;
        let action = action_label(decision.action()).to_owned();
        if !self.actions.contains(&action) {
            self.actions.push(action);
            self.actions.sort();
        }
        let Some(display) = masked_preview(decision.detection().kind(), value) else {
            return;
        };
        let duplicate = self
            .previews
            .iter()
            .any(|preview| preview.kind == kind && preview.display == display);
        let existing = self
            .previews
            .iter()
            .filter(|preview| preview.kind == kind)
            .count();
        if !duplicate && existing < 3 {
            self.previews.push(PrivacyPreview { kind, display });
        }
    }
}

pub(crate) fn entity_label(kind: &EntityKind) -> &'static str {
    match kind {
        EntityKind::ApiKey => "api_key",
        EntityKind::PrivateKey => "private_key",
        EntityKind::Password => "password",
        EntityKind::ConnectionString => "connection_string",
        EntityKind::Jwt => "jwt",
        EntityKind::ChinaResidentId => "china_resident_id",
        EntityKind::UnifiedSocialCreditCode => "unified_social_credit_code",
        EntityKind::BankCard => "bank_card",
        EntityKind::Phone => "phone",
        EntityKind::Email => "email",
        EntityKind::Ipv4 => "ipv4",
        EntityKind::Ipv6 => "ipv6",
        EntityKind::MacAddress => "mac_address",
        EntityKind::InternalHostname => "internal_hostname",
        EntityKind::Coordinate => "coordinate",
        EntityKind::Custom(_) => "custom",
    }
}

fn action_label(action: PolicyAction) -> &'static str {
    match action {
        PolicyAction::Allow => "allow",
        PolicyAction::Monitor => "monitor",
        PolicyAction::Block => "block",
        PolicyAction::Mask => "mask",
        PolicyAction::Pseudonymize => "pseudonymize",
    }
}

fn masked_preview(kind: &EntityKind, value: &str) -> Option<String> {
    match kind {
        EntityKind::ApiKey => mask_api_key(value),
        EntityKind::ChinaResidentId | EntityKind::UnifiedSocialCreditCode => {
            Some(mask_edges(value, 2, 2))
        }
        EntityKind::BankCard => Some(format!("****{}", trailing_chars(value, 4))),
        EntityKind::Ipv4 => mask_ipv4(value),
        EntityKind::Ipv6 => mask_components(value, ':'),
        EntityKind::MacAddress => mask_components(value, ':'),
        EntityKind::PrivateKey
        | EntityKind::Password
        | EntityKind::ConnectionString
        | EntityKind::Jwt
        | EntityKind::InternalHostname
        | EntityKind::Coordinate
        | EntityKind::Phone
        | EntityKind::Email
        | EntityKind::Custom(_) => None,
    }
}

fn mask_api_key(value: &str) -> Option<String> {
    let suffix = trailing_chars(value, 3);
    let prefix = value
        .find('-')
        .filter(|index| *index <= 4)
        .and_then(|index| value.get(..=index))
        .unwrap_or_default();
    Some(format!("{prefix}****{suffix}"))
}

fn mask_edges(value: &str, leading: usize, trailing: usize) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.len() <= leading + trailing {
        return "****".to_owned();
    }
    let start = characters.iter().take(leading).collect::<String>();
    let end = characters
        .iter()
        .skip(characters.len() - trailing)
        .collect::<String>();
    format!(
        "{start}{}{end}",
        "*".repeat(characters.len() - leading - trailing)
    )
}

fn trailing_chars(value: &str, count: usize) -> String {
    let mut characters = value.chars().rev().take(count).collect::<Vec<_>>();
    characters.reverse();
    characters.into_iter().collect()
}

fn mask_ipv4(value: &str) -> Option<String> {
    let parts = value.split('.').collect::<Vec<_>>();
    (parts.len() == 4).then(|| format!("{}.*.*.{}", parts[0], parts[3]))
}

fn mask_components(value: &str, separator: char) -> Option<String> {
    let parts = value
        .split(separator)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    (parts.len() >= 2).then(|| {
        format!(
            "{}{}*{}{}",
            parts[0],
            separator,
            separator,
            parts[parts.len() - 1]
        )
    })
}
