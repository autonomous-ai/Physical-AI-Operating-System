//! What a person teaches the lamp about how to behave with them (#813):
//! corrections and standing preferences, kept as short plain rules and
//! obeyed in every later session.
//!
//! Off unless the provider worker has `LAMP_MEMORY_RULES=on` and the Days
//! memory (`LAMP_MEMORY_DAYS=on`); without both the model is told of no new
//! function, the memory is asked nothing new and every note is as before.
//!
//! - **Three functions for the model** ([`tools`]): `keep_rule`,
//!   `forget_rule` and `list_rules`. Not tasks, reminders or facts: those
//!   stay `remember`.
//! - **Kept beside, never in, a person's memory**: the rules of the person
//!   with memory id `p-<hex>` are the memory's `kept` entries of scope
//!   `r-<hex>`, and the room's of `r-everyone` ([`scope`]). So a recall
//!   never returns a rule, and a rule is never told to anyone else.
//! - **Told to each new session** ([`note`]): the room's, and the known
//!   person's after them, within 600 bytes of rule text each.
//! - **A typed fact for the policy** ([`facts`], `lamp_msgs::Rules` on
//!   `memory/rules`): whether someone asked the lamp not to speak first,
//!   at all or in the mornings. Never the words of a rule.
//!
//! No word of a rule is ever logged.
use lamp_gemini::{Tool, ToolCall};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// Only the exact value `on` enables rules, and only with the Days memory.
/// The coordinator passes this one setting through its cleared worker
/// environment, like `LAMP_MEMORY_DAYS`.
pub const ENVIRONMENT: &str = "LAMP_MEMORY_RULES";

pub const KEEP_RULE: &str = "keep_rule";
pub const FORGET_RULE: &str = "forget_rule";
pub const LIST_RULES: &str = "list_rules";

/// The longest rule kept: one short sentence, never a person's whole words.
const RULE_BYTES: usize = 200;
/// The most rule text of one scope a session is told or a list gives.
const SCOPE_BYTES: u64 = 600;
/// What a rule about speaking first is marked with in the memory.
const QUIET: &str = "[quiet] ";
const QUIET_MORNINGS: &str = "[quiet_mornings] ";

static ON: OnceLock<bool> = OnceLock::new();
static BUS: OnceLock<PathBuf> = OnceLock::new();
static OUT: Mutex<Option<lamp_bus::Publisher>> = Mutex::new(None);

pub fn from_environment() -> bool {
    std::env::var(ENVIRONMENT).ok().as_deref() == Some("on")
}

/// Set once, by [`crate::remember::start`].
pub(crate) fn switch(on: bool) {
    let _ = ON.set(on);
}

pub fn on() -> bool {
    ON.get() == Some(&true)
}

/// Where the typed fact is published: the bus's root, when the voice has a
/// bus. Without it the rules still work; the policy is not told.
pub fn bus_on(root: &Path) {
    let _ = BUS.set(root.to_owned());
}

pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: KEEP_RULE.into(),
            description: "Call this only when the person corrects how you behave with them or tells you how they want \
                          you to behave from now on: what to call them, how long your answers should be, a topic to \
                          skip, when not to speak up, something you keep getting wrong. Not for tasks, reminders or \
                          things to remember: those are remember. Give the rule as one short plain sentence about \
                          what you will or will not do, such as 'Do not tell them the weather in the morning.', never \
                          their whole words. Then say back briefly what you will do differently."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {
                    "rule": {"type": "string", "description": "The rule, in one short sentence."},
                    "speaking_up": {"type": "string", "enum": ["never_first", "not_first_in_mornings"],
                        "description": "Only for a rule about you speaking first, unprompted: never_first if they asked you never to, not_first_in_mornings if only in the mornings. Leave out otherwise."}
                },
                "required": ["rule"]
            })),
            silent: false,
        },
        Tool {
            name: FORGET_RULE.into(),
            description: "Call this when the person takes back one of the rules they gave you, such as 'you can tell \
                          me the weather again' or 'forget that rule'. Give its number from your rules or from \
                          list_rules; leave it out for the rule they gave last. Then stop following it."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {"number": {"type": "integer", "description": "The rule's number."}}
            })),
            silent: false,
        },
        Tool {
            name: LIST_RULES.into(),
            description: "Call this when the person asks what they asked you to do or not to do, or how they asked \
                          you to behave. Tell them the rules in your own words, never their numbers."
                .into(),
            parameters: None,
            silent: false,
        },
    ]
}

pub fn is_tool(name: &str) -> bool {
    matches!(name, KEEP_RULE | FORGET_RULE | LIST_RULES)
}

/// The memory's scope for the rules of this person ([`crate::remember::person_id`]),
/// or of the room's for `None`: beside their memory, never in it.
pub fn scope(person: Option<&str>) -> String {
    let person = person.map_or("everyone", |id| id.strip_prefix("p-").unwrap_or(id));
    format!("r-{person}")
}

/// The memory's request for a call, or what to tell the model at once.
pub fn request(call: &ToolCall, now_unix_ms: u64, scope: &str) -> Result<Value, Value> {
    Ok(match call.name.as_str() {
        KEEP_RULE => {
            let rule = call
                .args
                .get("rule")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if !rule.chars().any(char::is_alphanumeric) {
                return Err(json!({"error": "There was no rule in that for me to keep."}));
            }
            if rule.len() > RULE_BYTES {
                return Err(
                    json!({"error": "That is too long for a rule: give it as one short sentence."}),
                );
            }
            let mark = match call.args.get("speaking_up").and_then(Value::as_str) {
                Some("never_first") => QUIET,
                Some("not_first_in_mornings") => QUIET_MORNINGS,
                _ => "",
            };
            json!({"op":"remember","person":scope,"at_unix_ms":now_unix_ms,"text":format!("{mark}{rule}")})
        }
        FORGET_RULE => match call
            .args
            .get("number")
            .and_then(Value::as_u64)
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        {
            Some(id) => json!({"op":"forget","person":scope,"id":id}),
            None => json!({"op":"forget_last","person":scope,"kind":"kept"}),
        },
        _ => json!({"op":"kept","person":scope,"max_bytes":SCOPE_BYTES}),
    })
}

/// One kept rule, without its mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub id: u64,
    pub at_unix_ms: u64,
    pub text: String,
    pub quiet: bool,
    pub quiet_mornings: bool,
}

/// The rules in the memory's answer to `kept`, oldest first.
pub fn rules(answer: &Value) -> Option<Vec<Rule>> {
    if answer["ok"] != true {
        return None;
    }
    answer
        .get("entries")?
        .as_array()?
        .iter()
        .map(|entry| {
            let text = entry.get("text")?.as_str()?;
            let (quiet, quiet_mornings, text) = if let Some(text) = text.strip_prefix(QUIET) {
                (true, false, text)
            } else if let Some(text) = text.strip_prefix(QUIET_MORNINGS) {
                (false, true, text)
            } else {
                (false, false, text)
            };
            Some(Rule {
                id: entry.get("id")?.as_u64()?,
                at_unix_ms: entry.get("at_unix_ms")?.as_u64()?,
                text: text.to_owned(),
                quiet,
                quiet_mornings,
            })
        })
        .collect()
}

/// What to tell the model for the memory's answer, and whether the rules
/// changed. `None` for an answer that is not one to that request. `when`
/// says how long ago a moment was, the way a person would.
pub fn answer_for(
    name: &str,
    answer: &Value,
    when: impl Fn(u64) -> String,
) -> Option<(Value, bool)> {
    if answer["ok"] != true {
        return match answer.get("error").and_then(Value::as_str)? {
            "full" if name == KEEP_RULE => Some((
                json!({"error": "I am keeping as many rules as I can. Ask which one they want me to drop."}),
                false,
            )),
            "empty" => Some((
                json!({"error": "There was no rule in that for me to keep."}),
                false,
            )),
            _ => None,
        };
    }
    Some(match name {
        KEEP_RULE => {
            answer.get("id")?.as_u64()?;
            (
                json!({"kept_rule": true, "note": "Follow it from now on, with them, without being reminded."}),
                true,
            )
        }
        FORGET_RULE => match answer.get("removed")? {
            Value::Null => (
                json!({"forgotten": false, "note": "There was no such rule."}),
                false,
            ),
            Value::Number(count) if count.as_u64() == Some(0) => (
                json!({"forgotten": false, "note": "There was no such rule."}),
                false,
            ),
            _ => (
                json!({"forgotten": true, "note": "Stop following it from now on."}),
                true,
            ),
        },
        _ => {
            let listed: Vec<Value> = rules(answer)?
                .iter()
                .map(|rule| json!({"number": rule.id, "rule": rule.text, "when": when(rule.at_unix_ms)}))
                .collect();
            if listed.is_empty() {
                (
                    json!({"rules": [], "note": "They have not asked you for anything like that."}),
                    false,
                )
            } else {
                (json!({ "rules": listed }), false)
            }
        }
    })
}

/// The room's rules, and the known person's when someone is, read before
/// `until`. `None` when either could not be had.
#[allow(clippy::type_complexity)]
pub fn read(
    ask: impl Fn(&Value, Duration) -> Option<Value>,
    person: Option<&str>,
    until: Instant,
) -> Option<(Vec<Rule>, Option<Vec<Rule>>)> {
    let one = |person: Option<&str>| {
        let left = until.checked_duration_since(Instant::now())?;
        rules(&ask(
            &json!({"op":"kept","person":scope(person),"max_bytes":SCOPE_BYTES}),
            left,
        )?)
    };
    let room = one(None)?;
    let theirs = match person {
        Some(person) => Some(one(Some(person))?),
        None => None,
    };
    Some((room, theirs))
}

/// What a session is told of the rules: nothing at all when there are
/// none. The rules `forget_rule` acts on (theirs, or the room's when
/// nobody is known) are numbered.
pub fn note(room: &[Rule], theirs: Option<&[Rule]>) -> String {
    if room.is_empty() && theirs.is_none_or(<[Rule]>::is_empty) {
        return String::new();
    }
    let mut note = String::from(
        "\n\nHow you were asked to behave\nThese are corrections and standing preferences people gave you about \
         how you behave with them. Follow them without being reminded and do not mention them unless asked; if \
         someone asks what they asked of you, call list_rules. A number in brackets is for forget_rule only: never \
         say it.\n",
    );
    let lines = |note: &mut String, rules: &[Rule], numbered: bool| {
        for rule in rules {
            note.push_str("- ");
            if numbered {
                note.push_str(&format!("({}) ", rule.id));
            }
            note.push_str(&rule.text);
            note.push('\n');
        }
    };
    match theirs {
        None => {
            note.push_str("Asked in this room:\n");
            lines(&mut note, room, true);
        }
        Some(theirs) => {
            if !room.is_empty() {
                note.push_str("Asked in this room by someone unknown:\n");
                lines(&mut note, room, false);
            }
            if !theirs.is_empty() {
                note.push_str(
                    "Asked by the person at the lamp as this conversation began, whose face you know:\n",
                );
                lines(&mut note, theirs, true);
            }
        }
    }
    note
}

/// The typed fact for the policy: the room's rules and theirs together.
pub fn facts(person: Option<&str>, room: &[Rule], theirs: &[Rule]) -> lamp_msgs::Rules {
    let all = || room.iter().chain(theirs);
    lamp_msgs::Rules {
        person: person.unwrap_or("everyone").to_owned(),
        quiet: all().any(|rule| rule.quiet),
        quiet_mornings: all().any(|rule| rule.quiet || rule.quiet_mornings),
        rules: u32::try_from(all().count()).unwrap_or(u32::MAX),
    }
}

/// Say the fact on `memory/rules`, kept until the next. Never waits for a
/// lock; a bus that cannot be used is said once and changes nothing else.
pub fn publish(facts: &lamp_msgs::Rules) {
    let Some(root) = BUS.get() else { return };
    let Ok(mut out) = OUT.try_lock() else { return };
    if out.is_none() {
        static SAID: OnceLock<()> = OnceLock::new();
        *out = lamp_bus::Bus::new(root)
            .and_then(|bus| {
                bus.publisher_declared(
                    lamp_msgs::Rules::TOPIC,
                    &lamp_msgs::Rules::declaration(),
                    crate::bus_out::SOURCE,
                )
            })
            .map_err(|error| {
                SAID.get_or_init(|| {
                    eprintln!("lamp-voice provider: rules are not published on the bus: {error}")
                });
            })
            .ok();
    }
    if let Some(publisher) = out.as_mut() {
        let _ = publisher.publish_json(0, facts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, args: Value) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            args,
        }
    }

    fn rule(id: u64, text: &str) -> Rule {
        rules(&json!({"ok":true,"entries":[{"id":id,"at_unix_ms":1,"text":text}]})).unwrap()[0]
            .clone()
    }

    #[test]
    fn a_rule_is_kept_beside_the_persons_memory_with_its_mark() {
        assert_eq!(scope(Some("p-be9688998f085098")), "r-be9688998f085098");
        assert_eq!(scope(None), "r-everyone");
        let keep = |args| request(&call(KEEP_RULE, args), 7, "r-x");
        assert_eq!(
            keep(json!({"rule":" Do not tell them the weather. ","speaking_up":"not_first_in_mornings"})).unwrap(),
            json!({"op":"remember","person":"r-x","at_unix_ms":7,"text":"[quiet_mornings] Do not tell them the weather."})
        );
        assert_eq!(
            keep(json!({"rule":"Call them Minh."})).unwrap()["text"],
            "Call them Minh."
        );
        assert!(keep(json!({"rule":" ... "})).is_err());
        assert!(keep(json!({"rule":"x".repeat(201)})).is_err());
        assert_eq!(
            request(&call(FORGET_RULE, json!({"number":3})), 7, "r-x").unwrap(),
            json!({"op":"forget","person":"r-x","id":3})
        );
        assert_eq!(
            request(&call(FORGET_RULE, json!({})), 7, "r-x").unwrap(),
            json!({"op":"forget_last","person":"r-x","kind":"kept"})
        );
        assert_eq!(
            request(&call(LIST_RULES, json!({})), 7, "r-x").unwrap(),
            json!({"op":"kept","person":"r-x","max_bytes":600})
        );
    }

    #[test]
    fn the_mark_becomes_the_typed_fact_and_never_reaches_the_model() {
        let morning = rule(1, "[quiet_mornings] Do not speak first in the morning.");
        assert_eq!(morning.text, "Do not speak first in the morning.");
        let name = rule(2, "Call them Minh.");
        let fact = facts(Some("p-1"), &[name], &[morning]);
        assert_eq!(
            fact,
            lamp_msgs::Rules {
                person: "p-1".into(),
                quiet: false,
                quiet_mornings: true,
                rules: 2
            }
        );
        let never = rule(3, "[quiet] Never speak first.");
        let fact = facts(None, &[never], &[]);
        assert!(fact.quiet && fact.quiet_mornings && fact.person == "everyone");
        let (listed, changed) = answer_for(
            LIST_RULES,
            &json!({"ok":true,"entries":[{"id":1,"at_unix_ms":1,"text":"[quiet_mornings] Do not speak first in the morning."}]}),
            |_| "today".into(),
        )
        .unwrap();
        assert!(!changed);
        assert_eq!(
            listed,
            json!({"rules":[{"number":1,"rule":"Do not speak first in the morning.","when":"today"}]})
        );
    }

    #[test]
    fn a_session_is_told_the_rooms_rules_and_only_the_known_persons_numbered() {
        assert_eq!(note(&[], None), "");
        assert_eq!(note(&[], Some(&[])), "");
        let room = [rule(4, "Answer in short sentences.")];
        let theirs = [rule(9, "Do not call them sir.")];
        let told = note(&room, Some(&theirs));
        assert!(told.contains("- Answer in short sentences.\n"), "{told}");
        assert!(told.contains("- (9) Do not call them sir.\n"), "{told}");
        assert!(note(&room, None).contains("- (4) Answer in short sentences.\n"));
    }

    /// The only test that names a bus: [`BUS`] is set once per process.
    #[test]
    fn the_fact_is_kept_on_the_bus_for_the_policy_to_read() {
        let root = PathBuf::from(format!("/tmp/lvr-rules-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        bus_on(&root);
        let fact = facts(
            Some("p-1"),
            &[],
            &[rule(
                1,
                "[quiet_mornings] Do not speak first in the morning.",
            )],
        );
        publish(&fact);
        let kept = lamp_bus::Bus::new(&root)
            .unwrap()
            .latest(lamp_msgs::Rules::TOPIC)
            .unwrap()
            .unwrap()
            .parse::<lamp_msgs::Rules>()
            .unwrap();
        assert_eq!(kept, fact);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn forgetting_says_whether_a_rule_went() {
        let forgot = |removed| {
            answer_for(FORGET_RULE, &json!({"ok":true,"removed":removed}), |_| {
                String::new()
            })
            .unwrap()
        };
        assert_eq!(forgot(json!(1)).0["forgotten"], true);
        assert!(forgot(json!(1)).1);
        assert_eq!(forgot(json!(0)).0["forgotten"], false);
        assert_eq!(forgot(json!(null)).0["forgotten"], false);
        assert_eq!(
            forgot(json!({"kind":"kept","id":2,"at_unix_ms":1})).0["forgotten"],
            true
        );
        assert!(
            answer_for(KEEP_RULE, &json!({"ok":false,"error":"storage"}), |_| {
                String::new()
            })
            .is_none()
        );
    }
}
