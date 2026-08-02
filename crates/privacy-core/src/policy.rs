use std::collections::{HashMap, HashSet};

use crate::EntityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyAction {
    Allow,
    Monitor,
    Block,
    Mask,
    Pseudonymize,
}

#[derive(Debug, Clone)]
pub struct Policy {
    default_action: PolicyAction,
    actions: HashMap<EntityKind, PolicyAction>,
    whitelist: HashSet<String>,
}

impl Policy {
    pub fn strict() -> Self {
        Self::with_defaults(PolicyAction::Pseudonymize, PolicyAction::Pseudonymize)
    }

    pub fn balanced() -> Self {
        Self::with_defaults(PolicyAction::Mask, PolicyAction::Monitor)
    }

    pub fn monitor() -> Self {
        Self::with_defaults(PolicyAction::Monitor, PolicyAction::Monitor)
    }

    pub fn with_whitelist<I, S>(mut self, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.whitelist = values
            .into_iter()
            .map(|value| value.as_ref().to_owned())
            .collect();
        self
    }

    pub fn action_for(&self, kind: &EntityKind) -> PolicyAction {
        self.actions
            .get(kind)
            .copied()
            .unwrap_or(self.default_action)
    }

    pub(crate) fn allows_value(&self, value: &str) -> bool {
        self.whitelist.contains(value)
    }

    fn with_defaults(default_action: PolicyAction, network_action: PolicyAction) -> Self {
        let mut actions = HashMap::new();
        for kind in [
            EntityKind::ApiKey,
            EntityKind::PrivateKey,
            EntityKind::Password,
            EntityKind::ConnectionString,
            EntityKind::Jwt,
        ] {
            actions.insert(kind, PolicyAction::Block);
        }
        for kind in [
            EntityKind::Ipv4,
            EntityKind::Ipv6,
            EntityKind::MacAddress,
            EntityKind::InternalHostname,
            EntityKind::Coordinate,
        ] {
            actions.insert(kind, network_action);
        }
        Self {
            default_action,
            actions,
            whitelist: HashSet::new(),
        }
    }
}
