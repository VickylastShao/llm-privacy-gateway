use std::sync::OnceLock;

use regex::Regex;

use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::add_detection;

#[derive(Debug, Default)]
pub struct PhoneDetector;

impl Detector for PhoneDetector {
    fn name(&self) -> &'static str {
        "phone"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();
        for matched in phone_regex().find_iter(text) {
            if has_digit_neighbor(text, matched.start(), matched.end()) {
                continue;
            }
            add_detection(
                &mut detections,
                EntityKind::Phone,
                matched.start(),
                matched.end(),
                100,
                "china_mobile_phone",
            )?;
        }
        Ok(detections)
    }
}

fn phone_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?:\+?86[ -]?)?1[3-9][0-9]{9}")
            .unwrap_or_else(|error| panic!("valid phone regex: {error}"))
    })
}

fn has_digit_neighbor(text: &str, start: usize, end: usize) -> bool {
    text.as_bytes()
        .get(start.wrapping_sub(1))
        .is_some_and(u8::is_ascii_digit)
        || text.as_bytes().get(end).is_some_and(u8::is_ascii_digit)
}
