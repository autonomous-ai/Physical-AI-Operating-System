//! A stop word heard while the lamp has stopped for someone (#828, step 2).
//!
//! The duplex rule stops the lamp's speech when someone is plainly over it,
//! and then knows only sound and timing: speech once the lamp is silent
//! commits the interruption, and stillness resumes the reply. A short
//! "Stop." is over by the time the lamp has stopped, so the rule took it
//! for "mm-hm" and went on. On lamp-4ace on 16 October at 05:39 the short
//! stop words did not stop the story in 2 of 4.
//!
//! With `LAMP_STOP_WORDS=heard` the words said in a yield are listened for,
//! with the second voice's speech-to-text ([`crate::second_config`]):
//! - when a reply begins, a stream is opened and kept ready;
//! - the yield's held audio and every block heard while it is in force go
//!   to it, and the rule waits for the words before stillness may resume
//!   the reply (its deadline still resumes it);
//! - once the speech has been still for [`FINALIZE_AFTER_STILL_BLOCKS`], the
//!   words are asked for. They came about 0.25 s after the end of the
//!   speech through the relay on lamp-52e6 (the sixth builder, 16 October);
//! - a stop phrase ([`stop_phrase`]) commits the yield, as speech would:
//!   the reply ends and what was said goes to the provider. Any other words
//!   let stillness resume it as before.
//!
//! Nothing is sent but what was said in a yield. Without the second voice's
//! configuration it is off.
use crate::second_config::SecondConfig;
use crate::second_provider::net::SttStream;
use std::{collections::VecDeque, io, sync::mpsc, time::Duration};
use tokio::sync::mpsc as queue;

/// Where the coordinator reads whether a yield's words are listened for.
/// `heard` only.
pub const ENVIRONMENT: &str = "LAMP_STOP_WORDS";

/// Whether the voice's environment says `heard`.
pub fn heard_on() -> bool {
    std::env::var(ENVIRONMENT).ok().as_deref() == Some("heard")
}

/// Blocks of stillness in a yield after which its words are asked for:
/// 100 ms, so that they come back inside the 400 ms after which stillness
/// would resume the reply.
pub const FINALIZE_AFTER_STILL_BLOCKS: u8 = 10;

/// Words that tell the lamp to stop, alone or among others: "Stop.",
/// "Wait, wait, wait.", "Okay, okay, that is enough.", "Lamp, stop."
const STOP_WORDS: &[&str] = &["stop", "wait", "enough", "pause", "quiet", "hush", "shush"];
/// Two words that do the same together: "Hold on.", "Hang on."
const STOP_PAIRS: &[(&str, &str)] = &[("hold", "on"), ("hang", "on")];

/// Whether what was heard tells the lamp to stop.
pub fn stop_phrase(heard: &str) -> bool {
    let words: Vec<String> = heard
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    words.iter().any(|word| STOP_WORDS.contains(&word.as_str()))
        || words
            .windows(2)
            .any(|pair| STOP_PAIRS.contains(&(pair[0].as_str(), pair[1].as_str())))
}

/// Where a yield's words come from.
pub trait Ears: Send {
    /// A reply has begun: have a stream ready for a yield in it.
    fn ready(&mut self);
    /// Audio said in the yield in force, in order.
    fn hear(&mut self, samples: &[i16]);
    /// The yield's speech has gone still: ask for its words.
    fn finalize(&mut self);
    /// The words, once they came: at most once a yield.
    fn words(&mut self) -> Option<String>;
    /// The yield is over: what it heard is dropped, and its words, if they
    /// come later, are not given.
    fn forget(&mut self);
    /// The reply is over: no stream is kept.
    fn rest(&mut self);
}

/// What the duplex rule is to be told after a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tell {
    Nothing,
    /// A yield began: it waits for its words.
    Await,
    /// Its words came: whether they were a stop phrase, and how many.
    Words {
        stop: bool,
        count: usize,
    },
    /// It ended before any words came.
    Unheard,
}

/// The yield's words, followed block by block.
pub struct StopWords {
    ears: Box<dyn Ears>,
    replying: bool,
    rested: bool,
    yielded: bool,
    asked: bool,
    told: bool,
}

impl StopWords {
    pub fn new(ears: Box<dyn Ears>) -> Self {
        Self {
            ears,
            replying: false,
            rested: true,
            yielded: false,
            asked: false,
            told: false,
        }
    }

    /// One capture block, after the rule has heard it. `replying`: a reply
    /// is under way. `yielded`: a yield is in force after this block, and
    /// `still` its blocks of stillness. `held`: the audio held for a yield
    /// that began at this block, this block's own among it; `samples`: this
    /// block's audio.
    pub fn block(
        &mut self,
        replying: bool,
        yielded: bool,
        still: Option<u8>,
        held: Option<&[crate::activity::ObservedAudio]>,
        samples: &[i16],
    ) -> Tell {
        if replying && !self.replying {
            self.ears.ready();
            self.rested = false;
        }
        self.replying = replying;
        let mut tell = Tell::Nothing;
        if yielded && !self.yielded {
            self.yielded = true;
            self.asked = false;
            self.told = false;
            match held {
                Some(held) => held.iter().for_each(|block| self.ears.hear(&block.samples)),
                None => self.ears.hear(samples),
            }
            tell = Tell::Await;
        } else if yielded {
            if !self.asked {
                self.ears.hear(samples);
            }
            if !self.asked && still.is_some_and(|still| still >= FINALIZE_AFTER_STILL_BLOCKS) {
                self.ears.finalize();
                self.asked = true;
            }
        } else if self.yielded {
            self.yielded = false;
            self.ears.forget();
            if !self.told {
                tell = Tell::Unheard;
            }
        }
        if !replying && !yielded && !self.rested {
            self.ears.rest();
            self.rested = true;
        }
        if self.yielded
            && !self.told
            && let Some(words) = self.ears.words()
        {
            self.told = true;
            tell = Tell::Words {
                stop: stop_phrase(&words),
                count: words.split_whitespace().count(),
            };
        }
        tell
    }

    /// The microphone was closed, by mute, privacy or a hand Stop: what a
    /// yield heard is dropped, and no stream is kept until the next reply
    /// (#828). Idempotent.
    pub fn close(&mut self) {
        if self.yielded {
            self.yielded = false;
            self.ears.forget();
        }
        if !self.rested {
            self.ears.rest();
            self.rested = true;
        }
        self.replying = false;
    }
}

/// The second voice's speech-to-text, on its own thread so that nothing on
/// the coordinator's path waits for the network.
pub struct RelayEars {
    commands: queue::UnboundedSender<Command>,
    heard: mpsc::Receiver<String>,
}

#[derive(Debug)]
enum Command {
    Ready,
    Hear(Vec<i16>),
    Finalize,
    Forget,
    Rest,
}

/// How long a failed connection waits before the next.
const RETRY: Duration = Duration::from_secs(5);
/// The most audio kept while no stream is ready: 3 s of 10 ms blocks.
const MOST_WAITING: usize = 300;

impl RelayEars {
    /// The speech-to-text `config` names, from a thread of its own.
    pub fn start(config: SecondConfig) -> io::Result<Self> {
        let (commands, queued) = queue::unbounded_channel();
        let (said, heard) = mpsc::channel();
        std::thread::Builder::new()
            .name("lamp-voice-stop-words".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(serve(config, queued, said)),
                    Err(error) => {
                        eprintln!("lamp-voice: stop words cannot be heard: {error}");
                    }
                }
            })?;
        Ok(Self { commands, heard })
    }

    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

impl Ears for RelayEars {
    fn ready(&mut self) {
        self.send(Command::Ready);
    }
    fn hear(&mut self, samples: &[i16]) {
        self.send(Command::Hear(samples.to_vec()));
    }
    fn finalize(&mut self) {
        self.send(Command::Finalize);
    }
    fn words(&mut self) -> Option<String> {
        self.heard.try_recv().ok()
    }
    fn forget(&mut self) {
        self.send(Command::Forget);
        while self.heard.try_recv().is_ok() {}
    }
    fn rest(&mut self) {
        self.send(Command::Rest);
    }
}

/// The thread's loop: a stream connected and kept ready while a reply is
/// under way, the yield's audio to it, and its words back once asked for.
async fn serve(
    config: SecondConfig,
    mut queued: queue::UnboundedReceiver<Command>,
    said: mpsc::Sender<String>,
) {
    let mut wanted = false;
    let mut ready: Option<SttStream> = None;
    let mut current: Option<SttStream> = None;
    let mut waiting: VecDeque<Vec<i16>> = VecDeque::new();
    let mut finalize_waiting = false;
    let mut connecting: Option<tokio::task::JoinHandle<Result<SttStream, String>>> = None;
    let mut finishing: Option<tokio::task::JoinHandle<Result<String, String>>> = None;
    let mut retry_at: Option<tokio::time::Instant> = None;
    let mut told_failure = false;
    loop {
        if wanted
            && ready.is_none()
            && connecting.is_none()
            && retry_at.is_none_or(|at| tokio::time::Instant::now() >= at)
        {
            let config = config.clone();
            connecting = Some(tokio::spawn(async move {
                SttStream::connect(&config)
                    .await
                    .map_err(|error| error.to_string())
            }));
        }
        tokio::select! {
            command = queued.recv() => match command {
                None => return,
                Some(Command::Ready) => wanted = true,
                Some(Command::Hear(samples)) => {
                    if current.is_none() {
                        current = ready.take();
                    }
                    match &current {
                        Some(stream) => {
                            for block in waiting.drain(..) {
                                let _ = stream.send(&block);
                            }
                            let _ = stream.send(&samples);
                        }
                        None => {
                            if waiting.len() == MOST_WAITING {
                                waiting.pop_front();
                            }
                            waiting.push_back(samples);
                        }
                    }
                }
                Some(Command::Finalize) => match current.take() {
                    Some(stream) => {
                        finishing = Some(tokio::spawn(async move {
                            stream.finished().await.map_err(|error| error.to_string())
                        }));
                    }
                    None => finalize_waiting = true,
                },
                Some(Command::Forget) => {
                    current = None;
                    waiting.clear();
                    finalize_waiting = false;
                    if let Some(finishing) = finishing.take() {
                        finishing.abort();
                    }
                }
                Some(Command::Rest) => {
                    wanted = false;
                    ready = None;
                    current = None;
                    waiting.clear();
                    finalize_waiting = false;
                    if let Some(task) = connecting.take() {
                        task.abort();
                    }
                    if let Some(task) = finishing.take() {
                        task.abort();
                    }
                }
            },
            connected = async { connecting.as_mut().expect("guarded").await }, if connecting.is_some() => {
                connecting = None;
                match connected {
                    Ok(Ok(stream)) => {
                        told_failure = false;
                        retry_at = None;
                        if current.is_none() && (!waiting.is_empty() || finalize_waiting) {
                            for block in waiting.drain(..) {
                                let _ = stream.send(&block);
                            }
                            if std::mem::take(&mut finalize_waiting) {
                                finishing = Some(tokio::spawn(async move {
                                    stream.finished().await.map_err(|error| error.to_string())
                                }));
                            } else {
                                current = Some(stream);
                            }
                        } else {
                            ready = Some(stream);
                        }
                    }
                    Ok(Err(error)) => {
                        if !told_failure {
                            eprintln!("lamp-voice: stop words: the speech-to-text cannot be reached, retried every {} s: {error}", RETRY.as_secs());
                            told_failure = true;
                        }
                        retry_at = Some(tokio::time::Instant::now() + RETRY);
                    }
                    Err(_) => {}
                }
            },
            finished = async { finishing.as_mut().expect("guarded").await }, if finishing.is_some() => {
                finishing = None;
                if let Ok(Ok(words)) = finished {
                    let _ = said.send(words);
                }
            },
            _ = tokio::time::sleep_until(retry_at.unwrap_or_else(tokio::time::Instant::now)), if retry_at.is_some() && connecting.is_none() => {
                retry_at = None;
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn stop_phrases_are_heard_alone_and_among_other_words() {
        for stop in [
            "Stop.",
            "Wait.",
            "Wait, wait, wait.",
            "Okay, okay, that is enough.",
            "Lamp, stop.",
            "Okay, stop.",
            "Hold on.",
            "Hang on, I have a question.",
            "Pause.",
            "Quiet, please.",
        ] {
            assert!(stop_phrase(stop), "{stop}");
        }
        for not in [
            "Mm-hm.",
            "Yeah.",
            "Wow.",
            "Tell me a joke instead.",
            "Hang",
            "On the other hand.",
            "",
        ] {
            assert!(!stop_phrase(not), "{not}");
        }
    }

    /// What the scripted ears were told, and the words they give.
    #[derive(Default)]
    struct Script {
        told: Vec<String>,
        words: Option<String>,
    }

    struct Scripted(Arc<Mutex<Script>>);

    impl Ears for Scripted {
        fn ready(&mut self) {
            self.0.lock().unwrap().told.push("ready".into());
        }
        fn hear(&mut self, samples: &[i16]) {
            self.0
                .lock()
                .unwrap()
                .told
                .push(format!("hear {}", samples[0]));
        }
        fn finalize(&mut self) {
            self.0.lock().unwrap().told.push("finalize".into());
        }
        fn words(&mut self) -> Option<String> {
            self.0.lock().unwrap().words.take()
        }
        fn forget(&mut self) {
            self.0.lock().unwrap().told.push("forget".into());
        }
        fn rest(&mut self) {
            self.0.lock().unwrap().told.push("rest".into());
        }
    }

    fn block(n: i16) -> [i16; 160] {
        [n; 160]
    }

    fn observed(n: i16) -> crate::activity::ObservedAudio {
        crate::activity::ObservedAudio {
            sequence: n as u64,
            captured_at_us: n as u64 * 10_000,
            samples: block(n),
        }
    }

    /// A reply; a yield with three blocks held; two blocks while paused and
    /// then stillness, its words asked for at the tenth still block and
    /// taken once; then the yield ends and the reply with it.
    #[test]
    fn a_yield_is_heard_from_its_held_audio_asked_once_and_forgotten() {
        let script = Arc::new(Mutex::new(Script::default()));
        let mut words = StopWords::new(Box::new(Scripted(Arc::clone(&script))));
        assert_eq!(
            words.block(true, false, None, None, &block(1)),
            Tell::Nothing
        );
        let held = [observed(2), observed(3), observed(4)];
        assert_eq!(
            words.block(true, true, Some(0), Some(&held), &block(4)),
            Tell::Await
        );
        assert_eq!(
            words.block(true, true, Some(0), None, &block(5)),
            Tell::Nothing
        );
        for still in 1..=10 {
            assert_eq!(
                words.block(true, true, Some(still), None, &block(6)),
                Tell::Nothing
            );
        }
        script.lock().unwrap().words = Some("Wait, wait.".into());
        assert_eq!(
            words.block(true, true, Some(11), None, &block(7)),
            Tell::Words {
                stop: true,
                count: 2
            }
        );
        script.lock().unwrap().words = Some("Stop.".into());
        assert_eq!(
            words.block(true, true, Some(12), None, &block(7)),
            Tell::Nothing
        );
        assert_eq!(
            words.block(true, false, None, None, &block(8)),
            Tell::Nothing
        );
        assert_eq!(
            words.block(false, false, None, None, &block(9)),
            Tell::Nothing
        );
        let told = script.lock().unwrap().told.clone();
        let mut expected = vec!["ready", "hear 2", "hear 3", "hear 4", "hear 5"];
        expected.extend(["hear 6"; 10]);
        expected.extend(["finalize", "forget", "rest"]);
        assert_eq!(told, expected);
    }

    /// A yield that ends before its words is said to be unheard. The
    /// microphone closing drops a yield and the stream at once, and only
    /// once, and the next reply has a stream ready again.
    #[test]
    fn a_yield_without_words_is_unheard_and_closing_drops_everything_once() {
        let script = Arc::new(Mutex::new(Script::default()));
        let mut words = StopWords::new(Box::new(Scripted(Arc::clone(&script))));
        words.block(true, false, None, None, &block(1));
        assert_eq!(
            words.block(true, true, Some(0), None, &block(2)),
            Tell::Await
        );
        assert_eq!(
            words.block(true, false, None, None, &block(3)),
            Tell::Unheard
        );
        assert_eq!(
            words.block(true, true, Some(0), None, &block(4)),
            Tell::Await
        );
        words.close();
        words.close();
        assert_eq!(
            words.block(true, false, None, None, &block(5)),
            Tell::Nothing
        );
        let told = script.lock().unwrap().told.clone();
        assert_eq!(
            told,
            [
                "ready", "hear 2", "forget", "hear 4", "forget", "rest", "ready"
            ]
        );
    }
}
