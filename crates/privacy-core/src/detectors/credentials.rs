use std::sync::OnceLock;

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use regex::Regex;
use serde_json::Value;

use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::{add_detection, sort_and_dedup};

const CONFIDENCE: u8 = 100;

#[derive(Debug, Default)]
pub struct CredentialsDetector;

impl Detector for CredentialsDetector {
    fn name(&self) -> &'static str {
        "credentials"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();

        for matched in vendor_api_key_regex().find_iter(text) {
            if has_safe_key_neighbor(text, matched.start(), matched.end()) {
                continue;
            }
            add_detection(
                &mut detections,
                EntityKind::ApiKey,
                matched.start(),
                matched.end(),
                CONFIDENCE,
                "vendor_api_key",
            )?;
        }

        for matched in jwt_regex().find_iter(text) {
            if has_safe_key_neighbor(text, matched.start(), matched.end())
                || !is_structurally_valid_jwt(matched.as_str())
            {
                continue;
            }
            add_detection(
                &mut detections,
                EntityKind::Jwt,
                matched.start(),
                matched.end(),
                CONFIDENCE,
                "jwt",
            )?;
        }

        for header in pem_header_regex().captures_iter(text) {
            let Some(header_match) = header.get(0) else {
                continue;
            };
            let Some(label) = header.get(1) else {
                continue;
            };
            let footer = format!("-----END {}-----", label.as_str());
            let body_start = header_match.end();
            let Some(footer_offset) = text[body_start..].find(&footer) else {
                continue;
            };
            let footer_start = body_start + footer_offset;
            if !is_valid_pem_body(&text[body_start..footer_start]) {
                continue;
            }
            add_detection(
                &mut detections,
                EntityKind::PrivateKey,
                header_match.start(),
                footer_start + footer.len(),
                CONFIDENCE,
                "private_key_pem",
            )?;
        }

        for captures in secret_assignment_regex().captures_iter(text) {
            let Some(key) = captures.name("key") else {
                continue;
            };
            let value = ["double", "single", "bare"]
                .iter()
                .find_map(|name| captures.name(name));
            let Some(value) = value else {
                continue;
            };
            let kind = if key.as_str().eq_ignore_ascii_case("password") {
                EntityKind::Password
            } else {
                EntityKind::ApiKey
            };
            add_detection(
                &mut detections,
                kind,
                value.start(),
                value.end(),
                CONFIDENCE,
                "secret_assignment",
            )?;
        }

        for matched in connection_string_regex().find_iter(text) {
            add_detection(
                &mut detections,
                EntityKind::ConnectionString,
                matched.start(),
                matched.end(),
                CONFIDENCE,
                "connection_string",
            )?;
        }

        sort_and_dedup(&mut detections);
        Ok(detections)
    }
}

fn vendor_api_key_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"sk-(?:proj-|ant-)?[A-Za-z0-9_-]{24,}")
            .unwrap_or_else(|error| panic!("valid vendor API key regex: {error}"))
    })
}

fn jwt_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}")
            .unwrap_or_else(|error| panic!("valid JWT regex: {error}"))
    })
}

fn is_structurally_valid_jwt(candidate: &str) -> bool {
    let mut segments = candidate.split('.');
    let (Some(header), Some(payload), Some(_signature), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };
    [header, payload].into_iter().all(is_base64url_json_object)
}

fn is_base64url_json_object(segment: &str) -> bool {
    let Ok(decoded) = URL_SAFE_NO_PAD.decode(segment) else {
        return false;
    };
    matches!(
        serde_json::from_slice::<Value>(&decoded),
        Ok(Value::Object(_))
    )
}

fn has_safe_key_neighbor(text: &str, start: usize, end: usize) -> bool {
    text.as_bytes()
        .get(start.wrapping_sub(1))
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        || text
            .as_bytes()
            .get(end)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn pem_header_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"-----BEGIN ([A-Z0-9 ]*PRIVATE KEY)-----")
            .unwrap_or_else(|error| panic!("valid PEM header regex: {error}"))
    })
}

fn is_valid_pem_body(body: &str) -> bool {
    let body = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'));
    let Some(body) = body else {
        return false;
    };
    if !body.ends_with('\n') {
        return false;
    }
    let lines: Vec<_> = body.lines().collect();
    if lines.is_empty() || lines.iter().any(|line| line.is_empty() || line.len() > 64) {
        return false;
    }
    let encoded = lines.concat();
    encoded
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'/' || byte == b'=')
        && STANDARD.decode(encoded).is_ok()
}

fn secret_assignment_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(?P<key>api_key|token|password|secret)\s*(?:=|:)\s*(?:\"(?P<double>[^\"\r\n]{8,})\"|'(?P<single>[^'\r\n]{8,})'|(?P<bare>[^\s\"']{8,}))"#,
        )
        .unwrap_or_else(|error| panic!("valid secret assignment regex: {error}"))
    })
}

fn connection_string_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)(?:postgresql|postgres|mysql|mongodb|redis|amqp)://[^\s/:@]+:[^\s@]+@[^\s"']+"#,
        )
        .unwrap_or_else(|error| panic!("valid connection string regex: {error}"))
    })
}
