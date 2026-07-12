use chrono::{Datelike, NaiveDate, Utc};
use regex::Regex;
use std::sync::OnceLock;

use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::{add_detection, sort_and_dedup};

const WEIGHTS: [u32; 17] = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
const CHECK_CODES: [u8; 11] = *b"10X98765432";

#[derive(Debug, Default)]
pub struct ChinaIdDetector;

impl Detector for ChinaIdDetector {
    fn name(&self) -> &'static str {
        "china_resident_id"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();
        for candidate in candidate_regex().find_iter(text) {
            if has_alphanumeric_neighbor(text, candidate.start(), candidate.end()) {
                continue;
            }
            if valid_resident_id(candidate.as_str()) {
                add_detection(
                    &mut detections,
                    EntityKind::ChinaResidentId,
                    candidate.start(),
                    candidate.end(),
                    100,
                    "china_resident_id",
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
        Regex::new(r"[A-Za-z0-9]{18}")
            .unwrap_or_else(|error| panic!("valid China resident ID regex: {error}"))
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

fn valid_resident_id(candidate: &str) -> bool {
    let bytes = candidate.as_bytes();
    if bytes.len() != 18 || !bytes[..17].iter().all(u8::is_ascii_digit) {
        return false;
    }
    let last = bytes[17].to_ascii_uppercase();
    if !last.is_ascii_digit() && last != b'X' {
        return false;
    }

    let year = match candidate[6..10].parse::<i32>() {
        Ok(year) => year,
        Err(_) => return false,
    };
    let month = match candidate[10..12].parse::<u32>() {
        Ok(month) => month,
        Err(_) => return false,
    };
    let day = match candidate[12..14].parse::<u32>() {
        Ok(day) => day,
        Err(_) => return false,
    };
    if !(1900..=Utc::now().year()).contains(&year)
        || NaiveDate::from_ymd_opt(year, month, day).is_none()
    {
        return false;
    }

    let checksum = bytes[..17]
        .iter()
        .zip(WEIGHTS)
        .map(|(digit, weight)| u32::from(digit - b'0') * weight)
        .sum::<u32>();
    CHECK_CODES[(checksum % 11) as usize] == last
}
