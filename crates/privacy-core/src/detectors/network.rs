use std::{net::IpAddr, sync::OnceLock};

use regex::Regex;

use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::{add_detection, sort_and_dedup};

#[derive(Debug, Default)]
pub struct NetworkDetector;

impl Detector for NetworkDetector {
    fn name(&self) -> &'static str {
        "network"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();

        for matched in email_regex().find_iter(text) {
            add_detection(
                &mut detections,
                EntityKind::Email,
                matched.start(),
                matched.end(),
                100,
                "email_regex",
            )?;
        }

        for matched in ip_candidate_regex().find_iter(text) {
            let Ok(address) = matched.as_str().parse::<IpAddr>() else {
                continue;
            };
            let kind = match address {
                IpAddr::V4(_) => EntityKind::Ipv4,
                IpAddr::V6(_) => EntityKind::Ipv6,
            };
            add_detection(
                &mut detections,
                kind,
                matched.start(),
                matched.end(),
                100,
                "ip_address",
            )?;
        }

        for matched in mac_regex().find_iter(text) {
            add_detection(
                &mut detections,
                EntityKind::MacAddress,
                matched.start(),
                matched.end(),
                100,
                "mac_address",
            )?;
        }

        for matched in hostname_regex().find_iter(text) {
            let followed_by_hostname_character = text
                .get(matched.end()..)
                .and_then(|remainder| remainder.chars().next())
                .is_some_and(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '.'
                });
            if followed_by_hostname_character {
                continue;
            }
            add_detection(
                &mut detections,
                EntityKind::InternalHostname,
                matched.start(),
                matched.end(),
                100,
                "internal_hostname",
            )?;
        }

        for captures in coordinate_regex().captures_iter(text) {
            let Some(latitude) = captures.name("lat") else {
                continue;
            };
            let Some(longitude) = captures.name("lon") else {
                continue;
            };
            let (Ok(latitude_value), Ok(longitude_value)) = (
                latitude.as_str().parse::<f64>(),
                longitude.as_str().parse::<f64>(),
            ) else {
                continue;
            };
            if latitude_value.abs() <= 90.0 && longitude_value.abs() <= 180.0 {
                add_detection(
                    &mut detections,
                    EntityKind::Coordinate,
                    latitude.start(),
                    longitude.end(),
                    100,
                    "coordinate_pair",
                )?;
            }
        }

        sort_and_dedup(&mut detections);
        Ok(detections)
    }
}

fn email_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)+")
            .unwrap_or_else(|error| panic!("valid email regex: {error}"))
    })
}

fn ip_candidate_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[0-9A-Fa-f:.]+")
            .unwrap_or_else(|error| panic!("valid IP candidate regex: {error}"))
    })
}

fn mac_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[0-9A-Fa-f]{2}(?::[0-9A-Fa-f]{2}){5}|[0-9A-Fa-f]{2}(?:-[0-9A-Fa-f]{2}){5}")
            .unwrap_or_else(|error| panic!("valid MAC address regex: {error}"))
    })
}

fn hostname_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)(?:[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?\.)+(?:internal|corp|local|lan)",
        )
        .unwrap_or_else(|error| panic!("valid internal hostname regex: {error}"))
    })
}

fn coordinate_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?P<lat>[+-]?\d{1,3}\.\d+)\s*,\s*(?P<lon>[+-]?\d{1,3}\.\d+)")
            .unwrap_or_else(|error| panic!("valid coordinate regex: {error}"))
    })
}
