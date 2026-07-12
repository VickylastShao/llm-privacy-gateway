use crate::{Detection, Detector, EntityKind, PrivacyError};

use super::{add_detection, sort_and_dedup};

#[derive(Debug, Default)]
pub struct BankCardDetector;

impl Detector for BankCardDetector {
    fn name(&self) -> &'static str {
        "bank_card_luhn"
    }

    fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let mut detections = Vec::new();
        let mut run_start = None;

        for (index, byte) in text.bytes().enumerate() {
            if byte.is_ascii_digit() || byte == b' ' || byte == b'-' {
                run_start.get_or_insert(index);
            } else if let Some(start) = run_start.take() {
                add_if_valid_card(text, start, index, &mut detections)?;
            }
        }
        if let Some(start) = run_start {
            add_if_valid_card(text, start, text.len(), &mut detections)?;
        }

        sort_and_dedup(&mut detections);
        Ok(detections)
    }
}

fn add_if_valid_card(
    text: &str,
    run_start: usize,
    run_end: usize,
    detections: &mut Vec<Detection>,
) -> Result<(), PrivacyError> {
    let candidate = text[run_start..run_end].trim_matches([' ', '-']);
    if candidate.is_empty() || !candidate.starts_with(|character: char| character.is_ascii_digit())
    {
        return Ok(());
    }
    let start = run_start
        + (text[run_start..run_end].len()
            - text[run_start..run_end]
                .trim_start_matches([' ', '-'])
                .len());
    let end = start + candidate.len();
    let digits: Vec<u8> = candidate.bytes().filter(u8::is_ascii_digit).collect();
    if !(12..=19).contains(&digits.len()) || digits.windows(2).all(|pair| pair[0] == pair[1]) {
        return Ok(());
    }
    if luhn_valid(&digits) {
        add_detection(
            detections,
            EntityKind::BankCard,
            start,
            end,
            100,
            "bank_card_luhn",
        )?;
    }
    Ok(())
}

fn luhn_valid(digits: &[u8]) -> bool {
    let sum = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(index, digit)| {
            let value = u32::from(digit - b'0');
            if index % 2 == 1 {
                let doubled = value * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                value
            }
        })
        .sum::<u32>();
    sum % 10 == 0
}
