//! What the physical skills did and why (`skills/events`, `docs/skills.md`).
//! Published by `lamp-skills` at each stage of a skill's run.
use crate::Invalid;
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};

/// A stage of one run of a skill, in the order they come.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillStage {
    /// A trigger's conditions held.
    Triggered,
    /// The Policy was asked whether the lamp may do it now.
    Asked,
    /// The Policy's answer, in `allowed`.
    Answered,
    /// The steps were run; `why` says how each went.
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillEvent {
    /// The skill's name, as its `SKILL.md` says: 1 to 64 lowercase letters,
    /// digits and hyphens.
    pub skill: String,
    pub stage: SkillStage,
    /// In words: what held, what was asked, what each step did.
    pub why: String,
    /// Whether the Policy let it go ahead. Only with `answered`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<bool>,
}

impl SkillEvent {
    pub const TYPE: &str = "lamp_msgs/SkillEvent";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "skills/events";

    /// Something that happened: no period, nothing kept.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event)
    }

    pub fn check(&self) -> Result<(), Invalid> {
        if self.skill.is_empty()
            || self.skill.len() > 64
            || !self
                .skill
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(Invalid(
                "a skill is 1..64 lowercase letters, digits or hyphens".into(),
            ));
        }
        if self.why.len() > 2_000 {
            return Err(Invalid("why is at most 2,000 bytes".into()));
        }
        if self.allowed.is_some() != (self.stage == SkillStage::Answered) {
            return Err(Invalid(
                "allowed comes with answered and only with it".into(),
            ));
        }
        Ok(())
    }
}

/// The skills part's optional topic, apart from the first topics.
pub fn skills_topics() -> Vec<(&'static str, Declaration)> {
    vec![(SkillEvent::TOPIC, SkillEvent::declaration())]
}
