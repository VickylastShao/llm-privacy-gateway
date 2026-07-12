use crate::{Detection, Detector, Policy, PolicyAction, PrivacyError, StaticDictionary, overlap};

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
}
