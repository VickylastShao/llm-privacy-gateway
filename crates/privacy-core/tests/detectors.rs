use std::{collections::HashSet, fs};

use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, Detector, EntityKind, NetworkDetector,
    UnifiedSocialCreditCodeDetector,
};
use serde::Deserialize;

fn assert_non_overlapping_and_in_bounds(text: &str, detections: &[privacy_core::Detection]) {
    let mut ranges: Vec<_> = detections
        .iter()
        .map(|detection| detection.range())
        .collect();
    ranges.sort_by_key(|range| range.start);
    for range in &ranges {
        assert!(
            text.get(range.clone()).is_some(),
            "range must be UTF-8 aligned"
        );
    }
    assert!(ranges.windows(2).all(|pair| pair[0].end <= pair[1].start));
}

fn assert_single(
    detector: &dyn Detector,
    text: &str,
    kind: EntityKind,
    source: &'static str,
    expected: &str,
) {
    let detections = match detector.detect(text) {
        Ok(detections) => detections,
        Err(error) => panic!("detector failed for {text}: {error}"),
    };
    assert_non_overlapping_and_in_bounds(text, &detections);
    assert_eq!(detections.len(), 1, "{text}");
    let detection = &detections[0];
    assert_eq!(detection.kind(), &kind);
    assert_eq!(detection.source(), source);
    assert_eq!(&text[detection.range()], expected);
}

fn assert_empty(detector: &dyn Detector, text: &str) {
    let detections = match detector.detect(text) {
        Ok(detections) => detections,
        Err(error) => panic!("detector failed for {text}: {error}"),
    };
    assert!(detections.is_empty(), "{text}");
}

#[test]
fn credentials_detect_explicit_key_jwt_pem_assignment_and_connection_string() {
    let detector = CredentialsDetector;
    assert_single(
        &detector,
        "sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890",
        EntityKind::ApiKey,
        "vendor_api_key",
        "sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890",
    );
    assert_single(
        &detector,
        "sk-ant-api03-abcdefghijklmnopqrstuvwx123456",
        EntityKind::ApiKey,
        "vendor_api_key",
        "sk-ant-api03-abcdefghijklmnopqrstuvwx123456",
    );
    assert_single(
        &detector,
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJmaXh0dXJlIn0.c2lnbmF0dXJl",
        EntityKind::Jwt,
        "jwt",
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJmaXh0dXJlIn0.c2lnbmF0dXJl",
    );
    assert_single(
        &detector,
        "-----BEGIN PRIVATE KEY-----\nRkFLRV9LRVlfTUFURVJJQUw=\n-----END PRIVATE KEY-----",
        EntityKind::PrivateKey,
        "private_key_pem",
        "-----BEGIN PRIVATE KEY-----\nRkFLRV9LRVlfTUFURVJJQUw=\n-----END PRIVATE KEY-----",
    );
    assert_single(
        &detector,
        "api_key = \"fixture_secret_123\"",
        EntityKind::ApiKey,
        "secret_assignment",
        "fixture_secret_123",
    );
    assert_single(
        &detector,
        "postgres://user:fixture_password@db.internal:5432/app",
        EntityKind::ConnectionString,
        "connection_string",
        "postgres://user:fixture_password@db.internal:5432/app",
    );
}

#[test]
fn credentials_reject_bare_high_entropy_or_noncredential_text() {
    let detector = CredentialsDetector;
    for text in [
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890",
        "a.b.c",
        "api_key is described in the documentation",
        "-----BEGIN PUBLIC KEY-----\nRkFLRQ==\n-----END PUBLIC KEY-----",
    ] {
        assert_empty(&detector, text);
    }
}

#[test]
fn credentials_require_jwt_json_header_and_payload() {
    assert_empty(&CredentialsDetector, "abcdefgh.abcdefgh.abcdefgh");
}

#[test]
fn credentials_require_pem_base64_body_lines() {
    assert_empty(
        &CredentialsDetector,
        "-----BEGIN PRIVATE KEY----- prose only -----END PRIVATE KEY-----",
    );
}

#[test]
fn credentials_detect_each_allowed_connection_string_scheme() {
    let detector = CredentialsDetector;
    for scheme in [
        "postgres",
        "postgresql",
        "mysql",
        "mongodb",
        "redis",
        "amqp",
    ] {
        let text = format!("{scheme}://user:fixture_password@db.internal:5432/app");
        assert_single(
            &detector,
            &text,
            EntityKind::ConnectionString,
            "connection_string",
            &text,
        );
    }
}

#[test]
fn china_id_requires_checksum_and_real_date() {
    let detector = ChinaIdDetector;
    assert_single(
        &detector,
        "证件号 11010519491231002X",
        EntityKind::ChinaResidentId,
        "china_resident_id",
        "11010519491231002X",
    );
    for text in ["110105194912310021", "110105199902300023"] {
        assert_empty(&detector, text);
    }
}

#[test]
fn uscc_requires_official_alphabet_and_checksum_without_losing_original_offsets() {
    let detector = UnifiedSocialCreditCodeDetector;
    let text = "company 91350211m000100y46";
    assert_single(
        &detector,
        text,
        EntityKind::UnifiedSocialCreditCode,
        "unified_social_credit_code",
        "91350211m000100y46",
    );
    for text in [
        "91350211M000100Y43",
        "91350211M000100Y45",
        "91350211I000100Y46",
    ] {
        assert_empty(&detector, text);
    }
}

#[test]
fn bank_cards_require_luhn_and_nonrepeated_digits() {
    let detector = BankCardDetector;
    assert_single(
        &detector,
        "pay 4111-1111 1111-1111 now",
        EntityKind::BankCard,
        "bank_card_luhn",
        "4111-1111 1111-1111",
    );
    for text in ["4111 1111 1111 1112", "1111 1111 1111 1111"] {
        assert_empty(&detector, text);
    }
}

#[test]
fn network_detector_validates_addresses_internal_hosts_macs_and_bounded_coordinates() {
    let detector = NetworkDetector;
    for (text, kind, source, expected) in [
        (
            "mail qa@example.test",
            EntityKind::Email,
            "email_regex",
            "qa@example.test",
        ),
        (
            "ip 192.168.1.44",
            EntityKind::Ipv4,
            "ip_address",
            "192.168.1.44",
        ),
        (
            "ip 2001:db8::7",
            EntityKind::Ipv6,
            "ip_address",
            "2001:db8::7",
        ),
        (
            "host api.service.internal",
            EntityKind::InternalHostname,
            "internal_hostname",
            "api.service.internal",
        ),
        (
            "mac 00:1A:2B:3C:4D:5E",
            EntityKind::MacAddress,
            "mac_address",
            "00:1A:2B:3C:4D:5E",
        ),
        (
            "coord 31.2304, 121.4737",
            EntityKind::Coordinate,
            "coordinate_pair",
            "31.2304, 121.4737",
        ),
    ] {
        assert_single(&detector, text, kind, source, expected);
    }
    for text in [
        "www.example.com",
        "host api.example.com",
        "host api.service.internality",
        "host api.corporate",
        "ip 999.1.1.1",
        "mac 00:1A:2B:3C:4D:5G",
        "coord 91.0, 121.0",
        "coord 31.0, 181.0",
    ] {
        assert_empty(&detector, text);
    }
}

#[test]
fn internal_hostname_suffix_must_end_the_hostname() {
    assert_empty(&NetworkDetector, "api.service.internal.example.com");
}

#[test]
fn mac_address_requires_boundaries_on_both_sides() {
    assert_empty(&NetworkDetector, "00:1A:2B:3C:4D:5E:6F");
}

#[test]
fn mac_address_rejects_left_hexadecimal_neighbor() {
    assert_empty(&NetworkDetector, "A00:1A:2B:3C:4D:5E");
}

#[test]
fn mac_address_rejects_hyphen_neighbors() {
    for text in ["-00:1A:2B:3C:4D:5E", "00:1A:2B:3C:4D:5E-"] {
        assert_empty(&NetworkDetector, text);
    }
}

#[derive(Debug, Deserialize)]
struct Fixture {
    id: String,
    text: String,
    expected: Vec<Expected>,
}

#[derive(Debug, Deserialize)]
struct Expected {
    kind: String,
    start: usize,
    end: usize,
}

fn fixture_detectors(id: &str) -> Vec<Box<dyn Detector>> {
    match id.split('-').next().unwrap_or_default() {
        "credentials" => vec![Box::new(CredentialsDetector)],
        "china" => vec![Box::new(ChinaIdDetector)],
        "uscc" => vec![Box::new(UnifiedSocialCreditCodeDetector)],
        "bank" => vec![Box::new(BankCardDetector)],
        "network" => vec![Box::new(NetworkDetector)],
        "benign" => vec![
            Box::new(CredentialsDetector),
            Box::new(ChinaIdDetector),
            Box::new(UnifiedSocialCreditCodeDetector),
            Box::new(BankCardDetector),
            Box::new(NetworkDetector),
        ],
        other => panic!("unknown fixture prefix: {other}"),
    }
}

fn fixture_kind(kind: &str) -> EntityKind {
    match kind {
        "ApiKey" => EntityKind::ApiKey,
        "BankCard" => EntityKind::BankCard,
        "ConnectionString" => EntityKind::ConnectionString,
        "ChinaResidentId" => EntityKind::ChinaResidentId,
        "Email" => EntityKind::Email,
        "Ipv4" => EntityKind::Ipv4,
        "Ipv6" => EntityKind::Ipv6,
        "Jwt" => EntityKind::Jwt,
        "MacAddress" => EntityKind::MacAddress,
        "PrivateKey" => EntityKind::PrivateKey,
        "InternalHostname" => EntityKind::InternalHostname,
        "Coordinate" => EntityKind::Coordinate,
        "UnifiedSocialCreditCode" => EntityKind::UnifiedSocialCreditCode,
        other => panic!("unknown fixture kind: {other}"),
    }
}

#[test]
fn evaluation_fixtures_have_correct_offsets_and_detector_results() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../privacy-evals");
    for filename in [
        "credentials.jsonl",
        "china-identifiers.jsonl",
        "bank-cards.jsonl",
        "network-identifiers.jsonl",
        "benign-lookalikes.jsonl",
    ] {
        let contents = match fs::read_to_string(format!("{root}/{filename}")) {
            Ok(contents) => contents,
            Err(error) => panic!("unable to read fixture {filename}: {error}"),
        };
        for line in contents.lines().filter(|line| !line.is_empty()) {
            let fixture: Fixture = match serde_json::from_str(line) {
                Ok(fixture) => fixture,
                Err(error) => panic!("invalid fixture JSON: {error}"),
            };
            let mut detections = Vec::new();
            for detector in fixture_detectors(&fixture.id) {
                match detector.detect(&fixture.text) {
                    Ok(mut detector_detections) => detections.append(&mut detector_detections),
                    Err(error) => panic!("detector failed for fixture {}: {error}", fixture.id),
                }
            }
            assert_non_overlapping_and_in_bounds(&fixture.text, &detections);
            let actual: HashSet<_> = detections
                .iter()
                .map(|detection| (detection.kind().clone(), detection.start(), detection.end()))
                .collect();
            let expected: HashSet<_> = fixture
                .expected
                .iter()
                .map(|expected| (fixture_kind(&expected.kind), expected.start, expected.end))
                .collect();
            assert_eq!(actual, expected, "fixture {}", fixture.id);
        }
    }
}
