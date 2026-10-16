//! A skill asks the policy whether the lamp may do something it would start
//! itself (`skills/ask`, #939, `docs/skills.md`): the policy is the one gate
//! for everything the lamp starts unprompted, and answers with a `Decision`
//! that names the ask (`Decision::ask`). Names, counts and times only: no
//! words of what would be said, no picture, no face.
use crate::Invalid;
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};

/// What a skill would do, as the policy judges it.
pub const ACTS: [&str; 3] = ["hello", "news", "unprompted"];

/// A skill asks "may the lamp do this now?".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillAsk {
    /// This ask's number, which the policy's decision names: the asker's
    /// monotonic clock in microseconds when it asked, so it grows and is
    /// never 0.
    pub ask: u64,
    /// The skill's name, as its folder is named: 1 to 64 lowercase letters,
    /// digits and hyphens.
    pub skill: String,
    /// What it would do ([`ACTS`]): `hello`, greet someone who has come
    /// back; `news`, tell news of work handed over; `unprompted`, anything
    /// else the lamp would start itself.
    pub act: String,
    /// The memory's id of the person it is for, if known (`p-` and 16 hex
    /// digits).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// For `hello`, and needed by it: how long nobody was at the lamp
    /// before, in whole minutes, and how long they have been there, in
    /// whole seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub away_min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub there_s: Option<u32>,
}

impl SkillAsk {
    pub const TYPE: &str = "lamp_msgs/SkillAsk";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "skills/ask";

    /// Something that happened and is over: no value is kept, and none is
    /// promised at any period.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Event)
    }

    pub fn check(&self) -> Result<(), Invalid> {
        if self.ask == 0 {
            return Err(Invalid("an ask's number is positive".into()));
        }
        if self.skill.is_empty()
            || self.skill.len() > 64
            || !self
                .skill
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(Invalid(
                "a skill's name is 1 to 64 lowercase letters, digits and hyphens".into(),
            ));
        }
        if !ACTS.contains(&self.act.as_str()) {
            return Err(Invalid("an act is hello, news or unprompted".into()));
        }
        if let Some(person) = &self.person
            && !(person.len() == 18
                && person.starts_with("p-")
                && person[2..]
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
        {
            return Err(Invalid("a person is p- and 16 hex digits".into()));
        }
        if self.act == "hello" && (self.away_min.is_none() || self.there_s.is_none()) {
            return Err(Invalid("a hello needs away_min and there_s".into()));
        }
        Ok(())
    }
}

/// The skills' optional topic, apart from the first topics.
pub fn skill_ask_topics() -> Vec<(&'static str, Declaration)> {
    vec![(SkillAsk::TOPIC, SkillAsk::declaration())]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> SkillAsk {
        SkillAsk {
            ask: 1_792_115_940_123_456,
            skill: "welcome-back".into(),
            act: "hello".into(),
            person: Some("p-be9688998f085098".into()),
            away_min: Some(42),
            there_s: Some(3),
        }
    }

    #[test]
    fn an_ask_is_checked_and_its_line_holds_no_words() {
        assert_eq!(hello().check(), Ok(()));
        let line = serde_json::to_string(&hello()).unwrap();
        assert_eq!(
            line,
            r#"{"ask":1792115940123456,"skill":"welcome-back","act":"hello","person":"p-be9688998f085098","away_min":42,"there_s":3}"#
        );
        let news = SkillAsk {
            act: "news".into(),
            skill: "task-news".into(),
            person: None,
            away_min: None,
            there_s: None,
            ..hello()
        };
        assert_eq!(news.check(), Ok(()));
        assert_eq!(
            serde_json::to_string(&news).unwrap(),
            r#"{"ask":1792115940123456,"skill":"task-news","act":"news"}"#
        );
        for wrong in [
            SkillAsk { ask: 0, ..hello() },
            SkillAsk {
                skill: String::new(),
                ..hello()
            },
            SkillAsk {
                skill: "Welcome_Back".into(),
                ..hello()
            },
            SkillAsk {
                act: "sing".into(),
                ..hello()
            },
            SkillAsk {
                person: Some("Minh".into()),
                ..hello()
            },
            SkillAsk {
                away_min: None,
                ..hello()
            },
            SkillAsk {
                there_s: None,
                ..hello()
            },
        ] {
            assert!(wrong.check().is_err(), "{wrong:?}");
        }
    }
}
