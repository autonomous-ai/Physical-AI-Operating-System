//! Faces the lamp was asked to remember (`docs/eyes.md`, "Who is there").
//!
//! The eyes keep the prints of the people who asked, and nobody else's. The
//! voice gives the model two functions that ask the eyes to remember or
//! forget the face in front of the lamp, and reads which of those people is
//! at the lamp from the eyes' small file, so that a hello and what a person
//! says can carry their name.
use lamp_gemini::{Tool, ToolCall};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// The eyes' request socket, from the voice service's environment.
pub const ENVIRONMENT: &str = "LAMP_EYES_WHO_SOCKET";
/// The provider worker's flag for it.
pub const FLAG: &str = "--who-socket";
pub const REMEMBER: &str = "remember_my_face";
pub const FORGET: &str = "forget_my_face";
/// Where the coordinator reads whether a name is checked before it is
/// kept (#829's names, the first builder at 13:24 on 16 October). `checked`
/// only.
pub const NAMES_ENVIRONMENT: &str = "LAMP_NAMES";
/// The provider worker's flag for it, right after the eyes' socket.
pub const NAMES_FLAG: &str = "--names-checked";
/// The person's own words of their last this many exchanges, with what is
/// being heard, count for a name: the request and the two before it before
/// the lamp's first words, the request and the one before it after them.
pub const SAID_EXCHANGES: usize = 2;

/// The provider worker's argument for the names switch.
pub fn names_arguments(value: Option<&str>) -> Vec<String> {
    if value == Some("checked") {
        vec![NAMES_FLAG.to_owned()]
    } else {
        Vec::new()
    }
}

/// Whether `name` is in what the person said (`said`): each of its words
/// among theirs, or its letters spelled one by one ("J-I-M", "J I M"). A
/// name the model made up from words heard in another language is not
/// (lamp-4ace, 16 October at 12:11: "No, my name is …" was heard as
/// Spanish words and kept as a name nobody said).
///
/// A word with an apostrophe is kept whole, so that "It's cold" spells no
/// "S", and a spelled run must be the name's word itself, not hold it ("I'm
/// a teacher" spells no "Ma"; the reviewer on #1057). Accents count for
/// nothing ("Jose" is "José", and "J O S E" spells it), and "double N" is
/// two of them ("A, double N" spells "Ann").
pub fn said_by_them(name: &str, said: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.replace('\u{2019}', "'")
            .split(|c: char| !(c.is_alphanumeric() || c == '\''))
            .map(|word| word.trim_matches('\''))
            .filter(|word| !word.is_empty())
            .map(|word| word.to_lowercase().chars().map(plain).collect())
            .collect()
    };
    let (name, theirs) = (words(name), words(said));
    let mut spelled: Vec<String> = Vec::new();
    let mut run = String::new();
    let mut doubled = false;
    for word in &theirs {
        if word == "double" {
            doubled = true;
            continue;
        }
        if word.chars().count() == 1 {
            run.push_str(word);
            if doubled {
                run.push_str(word);
            }
        } else if !run.is_empty() {
            spelled.push(std::mem::take(&mut run));
        }
        doubled = false;
    }
    if !run.is_empty() {
        spelled.push(run);
    }
    !name.is_empty()
        && name
            .iter()
            .all(|word| theirs.contains(word) || spelled.iter().any(|letters| letters == word))
}

/// Lower-case letters with their accents, by the letter without: the Latin
/// ones and Vietnamese's.
const ACCENTED: &[(&str, char)] = &[
    ("àáâãäåāăąảạằắẳẵặầấẩẫậ", 'a'),
    ("çćč", 'c'),
    ("ďđ", 'd'),
    ("èéêëēėęěẻẽẹềếểễệ", 'e'),
    ("ìíîïīįỉĩị", 'i'),
    ("ľłĺ", 'l'),
    ("ñńň", 'n'),
    ("òóôõöøōőơỏọồốổỗộờớởỡợ", 'o'),
    ("ŕř", 'r'),
    ("śšş", 's'),
    ("ťţ", 't'),
    ("ùúûüūůűưủũụừứửữự", 'u'),
    ("ýÿỳỷỹỵ", 'y'),
    ("źżž", 'z'),
];

/// `c` without its accent, when it has one ([`ACCENTED`]).
fn plain(c: char) -> char {
    ACCENTED
        .iter()
        .find(|(letters, _)| letters.contains(c))
        .map_or(c, |(_, plain)| *plain)
}

/// Capitalised words the service writes that are no one's name: "I'm OK.",
/// "I'm American.", "Call me Monday." (the reviewer on #1057).
const NOT_A_NAME: &[&str] = &[
    "i",
    "ok",
    "okay",
    "sorry",
    "fine",
    "good",
    "great",
    "here",
    "back",
    "done",
    "ready",
    "home",
    "lamp",
    "lamb",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
    // Not April, June or August: those are names too.
    "january",
    "february",
    "march",
    "july",
    "september",
    "october",
    "november",
    "december",
    "tomorrow",
    "today",
    "tonight",
    "american",
    "english",
    "british",
    "canadian",
    "australian",
    "vietnamese",
    "chinese",
    "japanese",
    "korean",
    "thai",
    "indian",
    "french",
    "german",
    "spanish",
    "italian",
    "mexican",
    "russian",
    "dutch",
    "swedish",
    "filipino",
    "indonesian",
    "brazilian",
    "portuguese",
];

/// Words that, just before "my name", make it a question about the name
/// and not the name given ("Do you know what my name is?"). Only these:
/// "Remember, my name is Jim." and "You know my name is Jim." give it (the
/// reviewer on #1057).
const ASKING: &[&str] = &["what", "what's", "whats"];

/// Question words that open a request and not a question about the name:
/// "Can you call me Jim?", "Could you remember that my name is Jim?".
const ASKING_FOR: &[&str] = &["can", "could", "will", "would"];

/// Words a question opens with: a sentence that opens with one and ends
/// with "?" asks, and gives no name.
const QUESTION: &[&str] = &[
    "do", "does", "did", "can", "could", "will", "would", "is", "are", "what", "who", "how", "why",
    "when", "where",
];

/// Whether what the person said gives their name: "my name is Jim", "my
/// name's Jim", "call me Jim", "I am called Jim", "I'm Jim" or "I am Jim",
/// with the name as the service writes one, capitalised and not a word that
/// is no one's name. Not after "what", and not in a question (the reviewer
/// on #1057: "Do you know what my name is, Lamp?"), except one asking the
/// lamp to call them so or to remember it ("Could you call me Jim?", "Do you
/// remember my name is Jim?").
pub fn gives_a_name(said: &str) -> bool {
    let said = said.replace('\u{2019}', "'");
    said.split_inclusive(['.', '!', '?']).any(gives_a_name_in)
}

fn gives_a_name_in(sentence: &str) -> bool {
    let words: Vec<&str> = sentence
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .filter(|word| !word.is_empty())
        .collect();
    let lower: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();
    if sentence.trim_end().ends_with('?')
        && lower.first().is_some_and(|first| {
            QUESTION.contains(&first.as_str()) && !ASKING_FOR.contains(&first.as_str())
        })
        && !lower.iter().any(|word| word == "remember")
    {
        return false;
    }
    let at = |i: usize| lower.get(i).map(String::as_str);
    let named = |i: usize| {
        words.get(i).is_some_and(|word| {
            word.chars().next().is_some_and(char::is_uppercase)
                && !NOT_A_NAME.contains(&word.to_lowercase().as_str())
        })
    };
    let asked = |i: usize| {
        i.checked_sub(1)
            .and_then(at)
            .is_some_and(|before| ASKING.contains(&before))
    };
    (0..lower.len()).any(|i| match (at(i), at(i + 1), at(i + 2)) {
        (Some("my"), Some("name"), Some("is")) => !asked(i) && named(i + 3),
        (Some("my"), Some("name's"), _) => !asked(i) && named(i + 2),
        (Some("call"), Some("me"), _) => named(i + 2),
        (Some("i"), Some("am"), Some("called")) => named(i + 3),
        (Some("i'm"), _, _) => named(i + 1),
        (Some("i"), Some("am"), _) => named(i + 2),
        _ => false,
    })
}

/// What the model is told beside a request in which the person gave their
/// name, with names checked: a name told is a name kept (the first builder,
/// #705, 13:43: on 15 October the lamp only said it would remember).
/// A condition, so that a match that was no name ("I'm Irish.") costs
/// nothing (the reviewer on #1057).
pub const NAME_TOLD_NOTE: &str = "If they have just told you their own name, call remember_my_face with it now, as they said or spelled it, or if you are not sure of it, say it back and ask them to spell it. Never only say that you will remember it. If what they said was not their name, do nothing about it.";

/// What the model is told when the name is not in what they said.
const NOT_HEARD: &str = "You did not hear that name in what they said. Say the name back to them and ask them to spell it, then call this again with the name as they said or spelled it.";
/// Remembering looks for up to four seconds.
pub const DEADLINE: Duration = Duration::from_secs(6);
/// The known file counts while it is this fresh: the eyes renew it every
/// two seconds while the person is there.
const FRESH: Duration = Duration::from_secs(3);
const MOST_NAME_BYTES: usize = 120;

/// The two functions the model is told of when the eyes remember faces.
/// With names checked, remembering takes `replace`, for a face known by
/// another name.
pub fn tools() -> Vec<Tool> {
    tools_checked(false)
}

/// The same, with names checked or not.
pub fn tools_checked(checked: bool) -> Vec<Tool> {
    vec![
        Tool {
            name: REMEMBER.into(),
            description: if checked {
                "Call this right away when someone tells you their name, or asks you to remember their face \
                 or who they are. It looks at them for a few seconds, so they should face you. Then say in a \
                 few words that you will know them now. Never only say that you will remember a name: this is \
                 how you do. If it says it could not see them, tell them why in a few words. Never call it with \
                 a name given for somebody else."
            } else {
                "Call this right away when someone asks you to remember their face or who they are, \
                 and has told you their name. It looks at them for a few seconds, so they should face you. \
                 Then say in a few words that you will know them now. If it says it could not see them, \
                 tell them why in a few words. Never call it for someone who did not ask, or with a name \
                 given for somebody else."
            }
            .into(),
            parameters: Some(if checked {
                json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Their name, as they said or spelled it."},
                        "replace": {"type": "boolean", "description": "Only when you were told this face is known by another name, and they said yes to being known by this one instead."}
                    },
                    "required": ["name"]
                })
            } else {
                json!({
                    "type": "object",
                    "properties": {"name": {"type": "string", "description": "Their name, as they said it."}},
                    "required": ["name"]
                })
            }),
            silent: false,
        },
        Tool {
            name: FORGET.into(),
            description: "Call this when someone asks you to forget their face or who they are, with the name \
                          you know them by. Then tell them briefly that you have forgotten it."
                .into(),
            parameters: Some(json!({
                "type": "object",
                "properties": {"name": {"type": "string", "description": "The name you know them by."}},
                "required": ["name"]
            })),
            silent: false,
        },
    ]
}

/// Whether the voice routes this function to the eyes.
pub fn is_tool(name: &str) -> bool {
    matches!(name, REMEMBER | FORGET)
}

/// Ask the eyes to remember or forget, and say what to tell the model; the
/// second value says whether the eyes answered. Blocking: for a thread of
/// its own. With names `checked`, a name the person did not say is not
/// kept, and a face known by another name is asked about first.
pub fn run(socket: &Path, call: &ToolCall, checked: bool) -> (Value, bool) {
    let said = if checked {
        crate::thread::THREAD
            .lock()
            .map(|thread| thread.said_lately(SAID_EXCHANGES))
            .unwrap_or_default()
    } else {
        String::new()
    };
    run_with(socket, call, checked, &said)
}

fn run_with(socket: &Path, call: &ToolCall, checked: bool, said: &str) -> (Value, bool) {
    let Some(name) = call
        .args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    else {
        return (
            json!({"error": "There was no name in that for me to keep."}),
            true,
        );
    };
    if checked && call.name == REMEMBER && !said_by_them(name, said) {
        return (json!({"error": NOT_HEARD}), true);
    }
    let replace = call.args.get("replace").and_then(Value::as_bool) == Some(true);
    let line = match call.name.as_str() {
        REMEMBER if checked => json!({"remember": name, "one_face": true, "replace": replace}),
        REMEMBER => json!({"remember": name}),
        FORGET => json!({"forget": name}),
        _ => return (crate::tools::unavailable(), false),
    };
    match crate::tools::exchange(socket, &line, DEADLINE) {
        Some(answer) => (told(&answer), true),
        None => (
            json!({"error": "My eye is not answering right now, so I could not do that."}),
            false,
        ),
    }
}

/// What the eyes said, in words the model can say.
fn told(answer: &Value) -> Value {
    if let Some(name) = answer.get("remembered").and_then(Value::as_str) {
        return json!({"ok": true, "remembered": name});
    }
    match answer.get("forgot").and_then(Value::as_bool) {
        Some(true) => return json!({"ok": true}),
        Some(false) => return json!({"error": "I did not know a face by that name."}),
        None => {}
    }
    if answer.get("error").and_then(Value::as_str) == Some("known_as")
        && let Some(known) = answer.get("known_as").and_then(Value::as_str)
    {
        return json!({"error": format!(
            "You already know this face as {known}. Say the new name back and ask whether they want to be known by it instead. Only if they say yes, call this again with the new name and replace true."
        )});
    }
    let words = match answer.get("error").and_then(Value::as_str) {
        Some("no_face") => "I could not see a face. They should look at me and ask again.",
        Some("too_far") => "Their face was too far away. They should come closer and ask again.",
        Some("two_faces") => {
            "There was more than one face in front of me. Only they should be in front of me when they ask again."
        }
        Some("eyes_shut") => "My eye is shut just now, so I could not see them.",
        Some("busy") => "I was already remembering someone. They should ask again in a moment.",
        Some("full") => {
            "I already know as many faces as I can keep. Someone has to be forgotten first."
        }
        Some("not_a_name") => "That is not a name I can keep.",
        _ => "My eye could not do that just now.",
    };
    json!({"error": words})
}

/// Who of the remembered people is at the lamp now: the name in the eyes'
/// file, written in the last three seconds.
pub fn known_now(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    if age > FRESH {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let name = text.trim();
    (!name.is_empty() && name.len() <= MOST_NAME_BYTES && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
}

/// The file the eyes keep the known person's name in while they are at the
/// lamp: `known`, beside their request socket (`lamp-eyes --known-file`).
pub fn known_file_of(socket: &Path) -> PathBuf {
    socket.with_file_name("known")
}

/// [`known_now`] for the eyes the environment names.
pub fn known_from_environment() -> Option<String> {
    let socket = std::env::var_os(ENVIRONMENT)?;
    let socket = Path::new(&socket);
    socket
        .is_absolute()
        .then(|| known_now(&known_file_of(socket)))
        .flatten()
}

/// A known person counts as gone only after this long without a fresh known
/// file. The eyes renew it within 3 s while they see the face, so a person
/// who looks away for a few seconds is still there; a new session for the
/// room is opened only once they have been unseen this long (#764).
pub const GONE_AFTER: Duration = Duration::from_secs(30);

/// A different known person counts only once the eyes have known them,
/// and not the one at the lamp, this long. On lamp-4ace on 16 October from
/// 16:06 the eyes flipped the owner between two names kept for one face,
/// and each flip opened a new session: the lamp was deaf to them (the first
/// builder, #705, 16:09). A person who sits down in someone's place keeps
/// on being known as themselves, so they count after this.
pub const SOMEONE_ELSE_AFTER: Duration = Duration::from_secs(10);

/// Who is at the lamp, as the voice counts it for whose session it is:
/// someone known counts at once when nobody was, or when the one counted
/// was last seen [`GONE_AFTER`] ago or more; a different known person only
/// after [`SOMEONE_ELSE_AFTER`] of being known with the one at the lamp
/// unseen; and nobody only after [`GONE_AFTER`]. It decides only whose
/// session it is: an exchange is the person's the eyes name as it is made
/// (`crate::thread::Thread::seen_now`).
#[derive(Default)]
pub struct Presence {
    who: Option<String>,
    seen_at: Option<Instant>,
    /// Someone else known, and since when, while the one at the lamp has
    /// not been seen.
    other: Option<(String, Instant)>,
}

impl Presence {
    /// What the known file said now (`None`: nobody fresh), and who is at
    /// the lamp after it.
    pub fn seen(&mut self, known: Option<String>, now: Instant) -> Option<&str> {
        // The one counted, unseen this long, is gone, whether or not it
        // was read between: the second route reads only as a turn opens,
        // and never sees the 30 s between turns (the reviewer on #1100).
        let gone = self
            .seen_at
            .is_some_and(|at| now.saturating_duration_since(at) >= GONE_AFTER);
        match known {
            Some(who) if !gone && self.who.as_ref().is_some_and(|at| *at != who) => {
                match &self.other {
                    Some((other, since))
                        if *other == who
                            && now.saturating_duration_since(*since) >= SOMEONE_ELSE_AFTER =>
                    {
                        self.who = Some(who);
                        self.seen_at = Some(now);
                        self.other = None;
                    }
                    Some((other, _)) if *other == who => {}
                    _ => self.other = Some((who, now)),
                }
            }
            Some(who) => {
                self.who = Some(who);
                self.seen_at = Some(now);
                self.other = None;
            }
            None if gone => self.who = None,
            None => {}
        }
        self.who.as_deref()
    }
}

/// What a request is told when someone the lamp knows is at it.
pub fn note(name: &str) -> String {
    format!("(The person at the lamp now is {name}: you know their face.)")
}

/// The hello for someone the lamp knows, in place of `speak_up::NOTE`.
pub fn hello(name: &str) -> String {
    format!(
        "(Nobody has spoken. {name}, whose face you know, has just come and sat down in front of you after being \
         away for a while. Say hello to them by name the way a friend at the next desk would: a few warm words and no \
         question they have to answer.)"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_eyes_say_becomes_words_the_model_can_say() {
        assert_eq!(
            told(&json!({"remembered":"Minh"})),
            json!({"ok":true,"remembered":"Minh"})
        );
        assert_eq!(told(&json!({"forgot":true})), json!({"ok":true}));
        assert!(told(&json!({"forgot":false}))["error"].is_string());
        for code in [
            "no_face",
            "too_far",
            "two_faces",
            "eyes_shut",
            "busy",
            "full",
            "not_a_name",
            "else",
        ] {
            let words = told(&json!({"error": code}));
            assert!(
                words["error"]
                    .as_str()
                    .is_some_and(|words| words.ends_with('.')),
                "{code}"
            );
        }
        assert_ne!(
            told(&json!({"error":"too_far"})),
            told(&json!({"error":"no_face"}))
        );
    }

    #[test]
    fn a_call_without_a_name_is_not_sent_and_a_silent_eye_is_said_so() {
        let call = |name: &str, args: Value| ToolCall {
            id: "1".into(),
            name: name.into(),
            args,
        };
        let nowhere = Path::new("/nonexistent/lamp-eyes-who.sock");
        let (words, answered) = run(nowhere, &call(REMEMBER, json!({"name":"  "})), false);
        assert!(words["error"].is_string() && answered);
        let (words, answered) = run(nowhere, &call(REMEMBER, json!({"name":"A"})), false);
        assert!(words["error"].as_str().unwrap().contains("eye") && !answered);
        assert!(is_tool(REMEMBER) && is_tool(FORGET) && !is_tool("remember"));
    }

    #[test]
    fn a_known_person_looking_away_is_still_there_and_another_counts_once_known_10_s() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut presence = Presence::default();
        assert_eq!(presence.seen(None, at(0)), None);
        assert_eq!(presence.seen(Some("a".into()), at(1)), Some("a"));
        // Looking away: still there until 30 s unseen.
        assert_eq!(presence.seen(None, at(5)), Some("a"));
        assert_eq!(presence.seen(None, at(30)), Some("a"));
        assert_eq!(presence.seen(Some("a".into()), at(31)), Some("a"));
        assert_eq!(presence.seen(None, at(60)), Some("a"));
        assert_eq!(presence.seen(None, at(61)), None);
        // Someone known counts as soon as the eyes know them when nobody was.
        assert_eq!(presence.seen(Some("a".into()), at(62)), Some("a"));
        // Someone else only once known 10 s with "a" unseen: a face flipping
        // between two names kept for it never counts as someone else.
        for second in [63, 66, 70, 72] {
            assert_eq!(presence.seen(Some("b".into()), at(second)), Some("a"));
        }
        assert_eq!(presence.seen(Some("a".into()), at(73)), Some("a"));
        assert_eq!(presence.seen(Some("b".into()), at(74)), Some("a"));
        assert_eq!(presence.seen(Some("b".into()), at(83)), Some("a"));
        assert_eq!(presence.seen(Some("b".into()), at(84)), Some("b"));
        // And "a" again, as someone else, only after 10 s.
        assert_eq!(presence.seen(Some("a".into()), at(85)), Some("b"));
        assert_eq!(presence.seen(Some("a".into()), at(95)), Some("a"));
        // The reviewer on #1100: read only as turns open, the one counted
        // last seen 30 s ago or more is gone, and someone else counts at
        // once. "b" at 3,600 s, an hour after "a".
        assert_eq!(presence.seen(Some("b".into()), at(95 + 3_600)), Some("b"));
        assert_eq!(presence.seen(Some("a".into()), at(95 + 3_629)), Some("b"));
        assert_eq!(presence.seen(Some("a".into()), at(95 + 3_630)), Some("a"));
    }

    #[test]
    fn the_known_name_counts_only_while_the_eyes_keep_it_fresh() {
        let path = std::env::temp_dir().join(format!("lamp-voice-known-{}", std::process::id()));
        assert_eq!(known_now(&path), None);
        std::fs::write(&path, "Minh\n").unwrap();
        assert_eq!(known_now(&path).as_deref(), Some("Minh"));
        let old = SystemTime::now() - Duration::from_secs(10);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(known_now(&path), None);
        std::fs::write(&path, "Minh\u{7}\n").unwrap();
        assert_eq!(known_now(&path), None);
        std::fs::remove_file(&path).unwrap();
        assert!(hello("Minh").contains("Minh") && note("Minh").contains("Minh"));
        assert_eq!(
            known_file_of(Path::new("/run/lampos-eyes/who.sock")),
            PathBuf::from("/run/lampos-eyes/known")
        );
    }

    #[test]
    fn a_name_given_is_told_from_talk_that_only_mentions_a_name() {
        for given in [
            "Hi Lamp, my name is Jim.",
            "My name's Jim.",
            "You can call me Jim.",
            "I'm Jim.",
            "Hello, I am Jim.",
            "I am called Jim.",
            // Curly apostrophes, as the service writes them too.
            "I\u{2019}m Jim.",
            "My name\u{2019}s Jim.",
            "Hi, my name is Jim, what's yours?",
            // The reviewer on #1057: the usual ways of asking to be
            // remembered give the name.
            "Remember, my name is Jim.",
            "Please remember my name is Jim.",
            "Don't forget, my name is Jim.",
            "You know my name is Jim.",
            "Can you call me Jim?",
            "Could you call me Jim from now on?",
            "Could you remember that my name is Jim?",
            "Do you remember my name is Jim?",
        ] {
            assert!(gives_a_name(given), "{given}");
        }
        for not in [
            "I'm tired.",
            "I am going home.",
            "Call me later.",
            "What is my name?",
            "Did Jim call?",
            "",
            // The reviewer on #1057: asking for the name, and capitalised
            // words that are no one's.
            "Do you know what my name is, Lamp?",
            "Tell me what my name is again.",
            "Why do you call me Jim?",
            "Did you call me Jim?",
            "I'm OK.",
            "I'm American.",
            "Call me Monday.",
        ] {
            assert!(!gives_a_name(not), "{not}");
        }
        assert!(
            tools_checked(true)[0]
                .description
                .contains("tells you their name")
        );
        assert!(
            !tools_checked(false)[0]
                .description
                .contains("tells you their name")
        );
    }

    #[test]
    fn a_name_counts_only_when_the_person_said_it_or_spelled_it() {
        assert!(said_by_them("Jim", "Hi, my name is Jim."));
        assert!(said_by_them("Jim", "It's J-I-M."));
        assert!(said_by_them("Jim", "j i m"));
        assert!(said_by_them("Mary Ann", "I'm Mary Ann"));
        assert!(said_by_them("Jo", "Jo. Just Jo."));
        // Made up from words heard in another language, or only the lamp's.
        assert!(!said_by_them("B", "No, mi nombre es y la"));
        assert!(!said_by_them("Mary Ann", "I'm Mary"));
        assert!(!said_by_them("Jim", "Yes."));
        assert!(!said_by_them("  ", "Jim"));
        // The reviewer on #1057: letters split off a word are not spelled,
        // and a spelled run is the name, not a part of one.
        for (name, said) in [
            ("S", "It's cold."),
            ("M", "I'm tired."),
            ("D", "I'd like tea."),
            ("Ma", "I'm a teacher."),
            ("Im", "J I M"),
        ] {
            assert!(!said_by_them(name, said), "{name}: {said}");
        }
        assert!(said_by_them("Jim", "It\u{2019}s J I M."));
        // The reviewer on #1057: accents count for nothing, and "double N"
        // is two.
        for (name, said) in [
            ("Jose", "My name is José."),
            ("José", "It's J O S E."),
            ("Zoe", "I'm Zoë."),
            ("Ann", "A, double N."),
            ("Phương", "Tôi là Phuong."),
        ] {
            assert!(said_by_them(name, said), "{name}: {said}");
        }
        assert!(!said_by_them("Anne", "A, double N."));
        assert_eq!(names_arguments(Some("checked")), [NAMES_FLAG]);
        assert!(names_arguments(Some("on")).is_empty());
        assert!(names_arguments(None).is_empty());
        assert_eq!(tools_checked(false), tools());
        assert!(
            tools_checked(true)[0].parameters.as_ref().unwrap()["properties"]["replace"]
                .is_object()
        );
    }

    /// The eyes stand-in on a real socket: what each request sent, and the
    /// answer it gives to each.
    fn eyes(answers: Vec<Value>) -> (PathBuf, std::thread::JoinHandle<Vec<Value>>) {
        use std::io::{BufRead, BufReader, Write};
        let directory = std::env::temp_dir().join(format!(
            "lamp-voice-names-{}-{}",
            std::process::id(),
            answers.len()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let socket = directory.join("who.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let asked = std::thread::spawn(move || {
            let mut asked = Vec::new();
            for answer in answers {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                asked.push(serde_json::from_str(&line).unwrap());
                let mut out = serde_json::to_vec(&answer).unwrap();
                out.push(b'\n');
                stream.write_all(&out).unwrap();
            }
            asked
        });
        (socket, asked)
    }

    /// The first builder's 13:24, with names checked: a name the person
    /// did not say goes nowhere and the model is told to ask for the
    /// spelling; a spelled one goes to the eyes as one name for one face; a
    /// face known by another name is said so; and `replace` goes once they
    /// said yes. Without the check, it is asked plainly, as before.
    #[test]
    fn with_names_checked_only_a_name_they_said_reaches_the_eyes_and_one_face_keeps_one() {
        let call = |args: Value| ToolCall {
            id: "1".into(),
            name: REMEMBER.into(),
            args,
        };
        let (socket, asked) = eyes(vec![
            json!({"error": "known_as", "known_as": "A"}),
            json!({"remembered": "Jim"}),
            json!({"remembered": "B"}),
        ]);
        let (words, answered) = run_with(
            &socket,
            &call(json!({"name": "B"})),
            true,
            "How do you spell my name? No, mi nombre es y la",
        );
        assert!(answered && words["error"].as_str().unwrap().contains("spell"));
        let (words, _) = run_with(&socket, &call(json!({"name": "Jim"})), true, "It's J I M.");
        assert!(words["error"].as_str().unwrap().contains("as A"), "{words}");
        let (words, _) = run_with(
            &socket,
            &call(json!({"name": "Jim", "replace": true})),
            true,
            "My name is Jim. Yes, call me Jim.",
        );
        assert_eq!(words, json!({"ok": true, "remembered": "Jim"}));
        let (words, _) = run_with(&socket, &call(json!({"name": "B"})), false, "");
        assert_eq!(words, json!({"ok": true, "remembered": "B"}));
        assert_eq!(
            asked.join().unwrap(),
            [
                json!({"remember": "Jim", "one_face": true, "replace": false}),
                json!({"remember": "Jim", "one_face": true, "replace": true}),
                json!({"remember": "B"}),
            ]
        );
        let _ = std::fs::remove_dir_all(socket.parent().unwrap());
    }
}
