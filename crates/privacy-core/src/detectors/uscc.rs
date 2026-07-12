use std::sync::OnceLock;

use regex::Regex;

use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::{add_detection, sort_and_dedup};

const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKLMNPQRTUWXY";
const WEIGHTS: [u32; 17] = [
    1, 3, 9, 27, 19, 26, 16, 17, 20, 29, 25, 13, 8, 24, 10, 30, 28,
];

#[derive(Debug, Default)]
pub struct UnifiedSocialCreditCodeDetector;

impl Detector for UnifiedSocialCreditCodeDetector {
    fn name(&self) -> &'static str {
        "unified_social_credit_code"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();
        for candidate in candidate_regex().find_iter(text) {
            if has_alphanumeric_neighbor(text, candidate.start(), candidate.end()) {
                continue;
            }
            if valid_uscc(candidate.as_str()) {
                add_detection(
                    &mut detections,
                    EntityKind::UnifiedSocialCreditCode,
                    candidate.start(),
                    candidate.end(),
                    100,
                    "unified_social_credit_code",
                )?;
            }
        }
        sort_and_dedup(&mut detections);
        Ok(detections)
    }
}

fn candidate_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9]{18}").unwrap_or_else(|error| panic!("valid USCC regex: {error}"))
    })
}

fn has_alphanumeric_neighbor(text: &str, start: usize, end: usize) -> bool {
    text.as_bytes()
        .get(start.wrapping_sub(1))
        .is_some_and(u8::is_ascii_alphanumeric)
        || text
            .as_bytes()
            .get(end)
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn valid_uscc(candidate: &str) -> bool {
    let upper = candidate.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    if bytes.len() != 18 {
        return false;
    }
    let mut sum = 0;
    for (character, weight) in bytes[..17].iter().zip(WEIGHTS) {
        let Some(value) = ALPHABET.iter().position(|allowed| allowed == character) else {
            return false;
        };
        sum += value as u32 * weight;
    }
    let check_value = (31 - sum % 31) % 31;
    ALPHABET
        .get(check_value as usize)
        .is_some_and(|expected| *expected == bytes[17])
}
