use session_map::Pseudonymizer;

use crate::{
    Detection, Detector, Policy, PolicyAction, PrivacyError, RequestPrivacyReport,
    StaticDictionary, overlap,
};

#[derive(Debug, Clone)]
pub struct Decision {
    detection: Detection,
    action: PolicyAction,
}

impl Decision {
    pub const fn detection(&self) -> &Detection {
        &self.detection
    }

    pub const fn action(&self) -> PolicyAction {
        self.action
    }
}

#[derive(Debug, Clone)]
pub struct Inspection {
    decisions: Vec<Decision>,
}

impl Inspection {
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    pub fn is_blocked(&self) -> bool {
        self.decisions
            .iter()
            .any(|decision| decision.action == PolicyAction::Block)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformResult {
    outbound_text: String,
}

impl TransformResult {
    #[must_use]
    pub fn outbound_text(&self) -> &str {
        &self.outbound_text
    }
}

pub struct PrivacyEngine {
    detectors: Vec<Box<dyn Detector>>,
    dictionary: StaticDictionary,
}

impl PrivacyEngine {
    pub fn new(detectors: Vec<Box<dyn Detector>>, dictionary: StaticDictionary) -> Self {
        Self {
            detectors,
            dictionary,
        }
    }

    pub fn inspect(&self, text: &str, policy: &Policy) -> Result<Inspection, PrivacyError> {
        let mut detections = self.dictionary.detect(text)?;
        for detector in &self.detectors {
            detections.extend(detector.detect(text)?);
        }
        let mut decisions = Vec::new();
        for detection in overlap::resolve(detections) {
            let value = text
                .get(detection.range())
                .ok_or(PrivacyError::InvalidTextRange)?;
            if !policy.allows_value(value) {
                decisions.push(Decision {
                    action: policy.action_for(detection.kind()),
                    detection,
                });
            }
        }
        Ok(Inspection { decisions })
    }

    pub fn transform<P: Pseudonymizer>(
        &self,
        text: &str,
        policy: &Policy,
        pseudonymizer: &mut P,
    ) -> Result<TransformResult, PrivacyError> {
        self.transform_reported(
            text,
            policy,
            pseudonymizer,
            &mut RequestPrivacyReport::default(),
        )
    }

    pub fn transform_reported<P: Pseudonymizer>(
        &self,
        text: &str,
        policy: &Policy,
        pseudonymizer: &mut P,
        report: &mut RequestPrivacyReport,
    ) -> Result<TransformResult, PrivacyError> {
        let inspection = self.inspect(text, policy)?;
        for decision in inspection.decisions() {
            let value = text
                .get(decision.detection().range())
                .ok_or(PrivacyError::InvalidTextRange)?;
            report.record(decision, value);
        }
        if inspection.is_blocked() {
            return Err(PrivacyError::Blocked);
        }

        let mut replacements = Vec::new();
        for decision in inspection.decisions() {
            let replacement = match decision.action() {
                PolicyAction::Pseudonymize => {
                    let value = text
                        .get(decision.detection().range())
                        .ok_or(PrivacyError::InvalidTextRange)?;
                    pseudonymizer
                        .pseudonymize(
                            crate::report::entity_label(decision.detection().kind()),
                            value,
                        )
                        .map_err(|_| PrivacyError::Pseudonymization)?
                }
                PolicyAction::Mask => "[REDACTED]".to_owned(),
                PolicyAction::Allow | PolicyAction::Monitor | PolicyAction::Block => continue,
            };
            replacements.push((decision.detection().range(), replacement));
        }

        let mut outbound_text = text.to_owned();
        for (range, replacement) in replacements.into_iter().rev() {
            outbound_text.replace_range(range, &replacement);
        }
        Ok(TransformResult { outbound_text })
    }
}
