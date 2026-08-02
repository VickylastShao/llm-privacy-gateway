use std::{collections::BTreeMap, fs, path::Path};

use privacy_core::{
    BankCardDetector, ChinaIdDetector, CredentialsDetector, NetworkDetector, PhoneDetector, Policy,
    PrivacyEngine, StaticDictionary, UnifiedSocialCreditCodeDetector,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Fixture {
    text: String,
    expected: Vec<ExpectedDetection>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Ord, PartialOrd)]
struct ExpectedDetection {
    kind: String,
    start: usize,
    end: usize,
}

#[derive(Debug, Default, Serialize)]
struct Counts {
    true_positive: usize,
    false_positive: usize,
    false_negative: usize,
}

#[derive(Debug, Serialize)]
struct Metric {
    precision: f64,
    recall: f64,
    f1: f64,
    true_positive: usize,
    false_positive: usize,
    false_negative: usize,
}

pub(crate) fn run(path: &Path) -> Result<std::process::ExitCode, String> {
    let mut counts = BTreeMap::<String, Counts>::new();
    for entry in fs::read_dir(path).map_err(|error| format!("cannot read eval path: {error}"))? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path
            .extension()
            .is_none_or(|extension| extension != "jsonl")
        {
            continue;
        }
        let content =
            fs::read_to_string(&path).map_err(|error| format!("cannot read fixture: {error}"))?;
        for line in content.lines().filter(|line| !line.trim().is_empty()) {
            let fixture: Fixture =
                serde_json::from_str(line).map_err(|_| "invalid JSONL fixture".to_owned())?;
            evaluate_fixture(&fixture, &mut counts)?;
        }
    }
    let metrics = counts
        .into_iter()
        .map(|(kind, count)| (kind, metric(count)))
        .collect::<BTreeMap<_, _>>();
    println!(
        "{}",
        serde_json::to_string(&metrics).map_err(|error| error.to_string())?
    );
    Ok(std::process::ExitCode::SUCCESS)
}

fn evaluate_fixture(
    fixture: &Fixture,
    counts: &mut BTreeMap<String, Counts>,
) -> Result<(), String> {
    let engine = PrivacyEngine::new(
        vec![
            Box::new(CredentialsDetector),
            Box::new(ChinaIdDetector),
            Box::new(UnifiedSocialCreditCodeDetector),
            Box::new(BankCardDetector),
            Box::new(PhoneDetector),
            Box::new(NetworkDetector),
        ],
        StaticDictionary::empty(),
    );
    let inspection = engine
        .inspect(&fixture.text, &Policy::strict())
        .map_err(|error| error.to_string())?;
    let mut actual = inspection
        .decisions()
        .iter()
        .map(|decision| {
            let detection = decision.detection();
            (
                format!("{:?}", detection.kind()),
                detection.start(),
                detection.end(),
            )
        })
        .collect::<Vec<_>>();
    actual.sort();
    let mut expected = fixture
        .expected
        .iter()
        .map(|item| (item.kind.clone(), item.start, item.end))
        .collect::<Vec<_>>();
    expected.sort();
    for item in &actual {
        let count = counts.entry(item.0.clone()).or_default();
        if expected.contains(item) {
            count.true_positive += 1;
        } else {
            count.false_positive += 1;
        }
    }
    for item in &expected {
        if !actual.contains(item) {
            counts.entry(item.0.clone()).or_default().false_negative += 1;
        }
    }
    Ok(())
}

fn metric(count: Counts) -> Metric {
    let precision = ratio(
        count.true_positive,
        count.true_positive + count.false_positive,
    );
    let recall = ratio(
        count.true_positive,
        count.true_positive + count.false_negative,
    );
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    Metric {
        precision,
        recall,
        f1,
        true_positive: count.true_positive,
        false_positive: count.false_positive,
        false_negative: count.false_negative,
    }
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}
