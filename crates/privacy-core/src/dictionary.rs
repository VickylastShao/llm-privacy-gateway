use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};

use crate::{Detection, EntityKind, PrivacyError};

#[derive(Debug)]
pub struct StaticDictionary {
    matcher: Option<AhoCorasick>,
    terms: Vec<String>,
}

impl StaticDictionary {
    pub fn empty() -> Self {
        Self {
            matcher: None,
            terms: Vec::new(),
        }
    }

    pub fn new<I, S>(terms: I) -> Result<Self, PrivacyError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut unique = Vec::new();
        for term in terms {
            let term = term.as_ref();
            if term.is_empty() {
                return Err(PrivacyError::EmptyDictionaryTerm);
            }
            if !unique.iter().any(|existing: &String| existing == term) {
                unique.push(term.to_owned());
            }
        }
        if unique.is_empty() {
            return Ok(Self::empty());
        }
        let matcher = AhoCorasickBuilder::new()
            .match_kind(MatchKind::Standard)
            .build(&unique)
            .map_err(|_| PrivacyError::DictionaryBuild)?;
        Ok(Self {
            matcher: Some(matcher),
            terms: unique,
        })
    }

    pub fn detect(&self, text: &str) -> Result<Vec<Detection>, PrivacyError> {
        let Some(matcher) = &self.matcher else {
            return Ok(Vec::new());
        };
        matcher
            .find_overlapping_iter(text)
            .map(|matched| {
                Detection::new(
                    EntityKind::Custom(self.terms[matched.pattern().as_usize()].clone()),
                    matched.start(),
                    matched.end(),
                    100,
                    "custom_dictionary",
                )
            })
            .collect()
    }
}
