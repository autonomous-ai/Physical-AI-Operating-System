//! The self-test's speaker check (#812): `lamp-selftest` asks the voice
//! (`selftest/speaker/request`) and the voice answers what its microphone
//! made of the lamp's own last speaking turn (`selftest/speaker`); nothing
//! is played. See `docs/speaker-check.md`.
use crate::{Invalid, invalid};
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A request for the speaker check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeakerCheckRequest {
    /// Names the request; the answer repeats it.
    pub id: String,
    /// `factory` or `on_demand`, the occasions it is asked at.
    pub occasion: String,
}

impl SpeakerCheckRequest {
    pub const TYPE: &str = "lamp_msgs/SpeakerCheckRequest";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "selftest/speaker/request";
    /// The occasions it is answered for.
    pub const OCCASIONS: [&str; 2] = ["factory", "on_demand"];

    /// Each request is its own; one not taken within 10 s is stale.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event).lasting(Duration::from_secs(10))
    }

    pub fn check(&self) -> Result<(), Invalid> {
        if self.id.is_empty() || self.id.len() > 64 {
            return invalid(format!("id {:?} is not 1 to 64 bytes", self.id));
        }
        if !Self::OCCASIONS.contains(&self.occasion.as_str()) {
            return invalid(format!(
                "the speaker check is answered only for {}, not {:?}",
                Self::OCCASIONS.join(" or "),
                self.occasion
            ));
        }
        Ok(())
    }
}

/// What the voice made of one request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeakerCheck {
    /// The request's id.
    pub id: String,
    /// `pass`, `fail` or `not_checked`, as the self-test's verdicts.
    pub verdict: String,
    /// Why, for a person. Empty for a pass.
    pub reason: String,
    /// The voice microphone's loudest level while the lamp spoke, over the
    /// room's level before it, before the echo canceller, in dB.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rise_db: Option<f32>,
    /// The echo path's delay. Measured only by the tone the check played
    /// until #968; now always absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u32>,
    /// How far the canceller took the sound down at its loudest, in dB:
    /// until #968 a tone's, at its peak; now the lamp's own speech, at the
    /// turn's loudest 10 ms block.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_db: Option<f32>,
    /// How far the canceller took the lamp's own speech down, in dB: the
    /// median per 10 ms block within 20 dB of the loudest of its second
    /// (`docs/speaker-check.md`). The figure the verdict is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_median_db: Option<f32>,
    /// Some measured blocks had the residual at the evidence's floor: the
    /// true figures are at least `cancelled_db` and `cancelled_median_db`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_lower_bound: Option<bool>,
    /// How long before the answer the measured turn ended, in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_age_s: Option<u32>,
}

impl SpeakerCheck {
    pub const TYPE: &str = "lamp_msgs/SpeakerCheck";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "selftest/speaker";
    pub const VERDICTS: [&str; 3] = ["pass", "fail", "not_checked"];

    /// The newest answer replaces the one before.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest)
    }

    pub fn check(&self) -> Result<(), Invalid> {
        if !Self::VERDICTS.contains(&self.verdict.as_str()) {
            return invalid(format!("no verdict {:?}", self.verdict));
        }
        if self.verdict != "pass" && self.reason.is_empty() {
            return invalid("a verdict other than pass says why".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_only_for_the_factory_or_when_asked() {
        let asked = SpeakerCheckRequest {
            id: "r1".into(),
            occasion: "on_demand".into(),
        };
        assert_eq!(asked.check(), Ok(()));
        let first_boot = SpeakerCheckRequest {
            occasion: "first_boot".into(),
            ..asked.clone()
        };
        assert!(first_boot.check().is_err());
        assert!(
            SpeakerCheckRequest {
                id: String::new(),
                ..asked
            }
            .check()
            .is_err()
        );
    }

    #[test]
    fn an_answer_has_a_verdict_and_says_why_unless_it_passed() {
        let passed = SpeakerCheck {
            id: "r1".into(),
            verdict: "pass".into(),
            reason: String::new(),
            rise_db: Some(24.5),
            delay_ms: Some(70),
            cancelled_db: Some(18.0),
            cancelled_median_db: Some(21.0),
            cancelled_lower_bound: Some(true),
            turn_age_s: Some(12),
        };
        assert_eq!(passed.check(), Ok(()));
        let text = serde_json::to_string(&passed).unwrap();
        assert_eq!(serde_json::from_str::<SpeakerCheck>(&text).unwrap(), passed);
        let silent = SpeakerCheck {
            verdict: "not_checked".into(),
            rise_db: None,
            delay_ms: None,
            cancelled_db: None,
            cancelled_median_db: None,
            cancelled_lower_bound: None,
            turn_age_s: None,
            ..passed.clone()
        };
        assert!(silent.check().is_err());
        assert!(
            SpeakerCheck {
                verdict: "maybe".into(),
                ..passed
            }
            .check()
            .is_err()
        );
    }
}
