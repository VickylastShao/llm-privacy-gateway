use std::ops::Range;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityKind {
    ApiKey,
    PrivateKey,
    Password,
    ConnectionString,
    Jwt,
    ChinaResidentId,
    UnifiedSocialCreditCode,
    BankCard,
    Phone,
    Email,
    Ipv4,
    Ipv6,
    MacAddress,
    InternalHostname,
    Coordinate,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    kind: EntityKind,
    start: usize,
    end: usize,
    confidence: u8,
    source: &'static str,
}

impl Detection {
    pub fn new(
        kind: EntityKind,
        start: usize,
        end: usize,
        confidence: u8,
        source: &'static str,
    ) -> Result<Self, PrivacyError> {
        if start >= end {
            return Err(PrivacyError::InvalidRange { start, end });
        }
        if confidence > 100 {
            return Err(PrivacyError::InvalidConfidence { confidence });
        }

        Ok(Self {
            kind,
            start,
            end,
            confidence,
            source,
        })
    }

    #[must_use]
    pub const fn kind(&self) -> &EntityKind {
        &self.kind
    }

    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }

    #[must_use]
    pub const fn confidence(&self) -> u8 {
        self.confidence
    }

    #[must_use]
    pub const fn source(&self) -> &'static str {
        self.source
    }

    #[must_use]
    pub const fn range(&self) -> Range<usize> {
        self.start..self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PrivacyError {
    #[error("invalid detection range: start ({start}) must be less than end ({end})")]
    InvalidRange { start: usize, end: usize },
    #[error("invalid detection confidence: {confidence} exceeds 100")]
    InvalidConfidence { confidence: u8 },
    #[error("dictionary terms must not be empty")]
    EmptyDictionaryTerm,
    #[error("dictionary matcher could not be constructed")]
    DictionaryBuild,
    #[error("detection range does not align with the inspected text")]
    InvalidTextRange,
}
