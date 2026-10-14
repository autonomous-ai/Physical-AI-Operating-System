//! The executive: what the lamp can be asked to do to itself, and the doing.
//!
//! The voice's model asks for a function by name (docs/tools.md). It never
//! supplies anything else: a name, and for two functions one word out of a
//! closed list. Each name maps to steps this code owns. Before every step
//! the present state is read; after it the state is read again; and what
//! goes back to the model is what was established, in words it can say:
//! "The volume is now 60 out of 100", "The light is already at its
//! brightest", "I could not confirm it changed". A command that was tried
//! is finished: nothing is retried and nothing is handed on.
//!
//! The executive owns no conversation and no microphone. It writes the
//! speaker's volume control, which nothing else writes, and it asks the
//! ring's owner for a light over the bus (`light/command`), reading back
//! what that owner says it shows (`light/state`). A function whose
//! hardware is not there is not offered: the list handed to the voice
//! (`tools.json`) holds only what can be done now.
//!
//! What is kept of V1 and what is not is in docs/actions.md. In short:
//! the idea is kept (the model names a command, the code owns the step,
//! every change is read back, and the result is said as it is); the sound
//! steps are this crate's own; the light's arithmetic is still V1's and
//! is to be replaced by a colour and a level when the ring's owner is
//! built.

pub mod light;
pub mod schedule;
pub mod server;
pub mod sound;

use serde_json::{Value, json};

/// A function as the voice's model is told of it (docs/tools.md).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Function {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

/// What became of one call: said to the model either way.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    /// What was established, as a sentence.
    Done(String),
    /// Why it was not done, or could not be confirmed, as a sentence.
    Failed(String),
}
impl Outcome {
    /// The line that answers the call.
    pub fn line(&self) -> Value {
        match self {
            Self::Done(said) => json!({"ok": true, "result": {"result": said}}),
            Self::Failed(said) => json!({"ok": false, "error": said}),
        }
    }
}

/// The functions that can be carried out now.
pub fn catalog(sound: bool, light: bool) -> Vec<Function> {
    let plain = |name, description| Function {
        name,
        description,
        parameters: None,
    };
    let mut functions = Vec::new();
    if sound {
        functions.extend([
            plain(
                "volume_up",
                "Make your own speaker louder by one step, when someone asks you to speak up or says you are too quiet.",
            ),
            plain(
                "volume_down",
                "Make your own speaker quieter, when someone asks you to keep it down or says you are too loud.",
            ),
            plain(
                "volume_mute",
                "Silence your own speaker until you are asked to unmute it.",
            ),
            plain("volume_unmute", "Bring your own speaker back after it was muted."),
        ]);
    }
    if light {
        let names = lamp_msgs::Light::COLOURS;
        functions.extend([
            plain(
                "light_on",
                "Switch your own light on, when someone asks for light and names no colour.",
            ),
            plain("light_off", "Switch your own light off."),
            Function {
                name: "light_colour",
                description: "Set your own light to one of the named colours. Violet is purple; warm white is warm.",
                parameters: Some(json!({
                    "type": "object",
                    "properties": {"colour": {"type": "string", "enum": names}},
                    "required": ["colour"]
                })),
            },
            plain(
                "light_brighter",
                "Make your own light a step brighter, keeping its colour.",
            ),
            plain(
                "light_dimmer",
                "Make your own light a step dimmer, keeping its colour, when someone says it is too bright or harsh.",
            ),
        ]);
    }
    functions
}

/// The list as the voice reads it.
pub fn catalog_file(functions: &[Function]) -> String {
    json!({"tools": functions}).to_string()
}

/// The hardware the executive may use, each part there or not.
pub struct Executive<S, L> {
    pub sound: Option<sound::Volume<S>>,
    pub light: Option<L>,
}

impl<S: sound::Mixer, L: light::Light> Executive<S, L> {
    /// What can be done now.
    pub fn offered(&mut self) -> Vec<Function> {
        let sound = self.sound.is_some();
        let light = self.light.as_mut().is_some_and(|light| light.there());
        catalog(sound, light)
    }

    /// Carry out one call. A name that is not offered, and anything but the
    /// one word a function takes, is refused before any hardware is read.
    pub fn call(&mut self, name: &str, args: &Value) -> Outcome {
        let nothing = args.as_object().is_none_or(serde_json::Map::is_empty) || args.is_null();
        let unknown = || Outcome::Failed("I can't do that.".into());
        match name {
            "volume_up" | "volume_down" | "volume_mute" | "volume_unmute" if nothing => {
                let Some(volume) = self.sound.as_mut() else {
                    return unknown();
                };
                match name {
                    "volume_up" => volume.up(),
                    "volume_down" => volume.down(),
                    "volume_mute" => volume.mute(),
                    _ => volume.unmute(),
                }
            }
            "light_on" | "light_off" | "light_brighter" | "light_dimmer" if nothing => {
                let Some(light) = self.light.as_mut() else {
                    return unknown();
                };
                if !light.there() {
                    return unknown();
                }
                match name {
                    "light_on" => light::on(light),
                    "light_off" => light::off(light),
                    "light_brighter" => light::brighter(light),
                    _ => light::dimmer(light),
                }
            }
            "light_colour" => {
                let Some(light) = self.light.as_mut() else {
                    return unknown();
                };
                if !light.there() {
                    return unknown();
                }
                // One word, and one of the ten.
                let asked = args
                    .as_object()
                    .filter(|object| object.len() == 1)
                    .and_then(|object| object.get("colour"))
                    .and_then(Value::as_str);
                match asked {
                    Some(name) => light::colour(light, name),
                    None => Outcome::Failed("That is not a colour my light can show.".into()),
                }
            }
            _ => unknown(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_can_be_done_now_is_offered_and_every_function_is_one_the_voice_accepts() {
        assert!(catalog(false, false).is_empty());
        let sound: Vec<&str> = catalog(true, false).iter().map(|f| f.name).collect();
        assert_eq!(
            sound,
            ["volume_up", "volume_down", "volume_mute", "volume_unmute"]
        );
        let all = catalog(true, true);
        assert_eq!(all.len(), 9);
        // The bounds the provider link puts on a function (docs/tools.md).
        for function in &all {
            let mut name = function.name.bytes();
            assert!(name.next().is_some_and(|first| first.is_ascii_lowercase()));
            assert!(name.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'));
            assert!(function.name.len() <= 48 && function.description.len() <= 1_024);
            assert!(
                function
                    .parameters
                    .as_ref()
                    .is_none_or(|p| p.is_object() && p.to_string().len() <= 4_096)
            );
        }
        // The file is the shape the voice reads, and only one function
        // takes anything: a colour out of the ten.
        let file: Value = serde_json::from_str(&catalog_file(&all)).unwrap();
        let listed = file["tools"].as_array().unwrap();
        assert_eq!(listed.len(), 9);
        let taking: Vec<&Value> = listed
            .iter()
            .filter(|f| f.get("parameters").is_some())
            .collect();
        assert_eq!(taking.len(), 1);
        assert_eq!(
            taking[0]["parameters"]["properties"]["colour"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        assert!(catalog_file(&all).len() < 65_536);
    }

    #[test]
    fn an_outcome_is_one_line_the_voice_can_hand_to_the_model() {
        assert_eq!(
            Outcome::Done("The volume is now 60 out of 100.".into()).line(),
            json!({"ok":true,"result":{"result":"The volume is now 60 out of 100."}})
        );
        assert_eq!(
            Outcome::Failed("I could not confirm it changed.".into()).line(),
            json!({"ok":false,"error":"I could not confirm it changed."})
        );
    }
}
