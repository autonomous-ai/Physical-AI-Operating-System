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
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex, PoisonError, mpsc},
    time::Duration,
};
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

/// Words that, just before a stop word, turn it round: "Don't stop.", "No,
/// never stop." (the reviewer on #970).
const NOT: &[&str] = &[
    "don't", "dont", "not", "never", "doesn't", "didn't", "won't", "can't", "cannot",
];

/// Whether what was heard tells the lamp to stop.
pub fn stop_phrase(heard: &str) -> bool {
    let words: Vec<String> = heard
        .replace('\u{2019}', "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(|word| word.trim_matches('\''))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    let turned = |at: usize| at > 0 && NOT.contains(&words[at - 1].as_str());
    words
        .iter()
        .enumerate()
        .any(|(at, word)| STOP_WORDS.contains(&word.as_str()) && !turned(at))
        || words.windows(2).enumerate().any(|(at, pair)| {
            STOP_PAIRS.contains(&(pair[0].as_str(), pair[1].as_str())) && !turned(at)
        })
}

/// Where a yield's words come from.
pub trait Ears: Send {
    /// A reply has begun: have a stream ready for a yield in it.
    fn ready(&mut self);
    /// Audio said in the yield in force, in order, with when it was
    /// captured.
    fn hear(&mut self, captured_at_us: u64, samples: &[i16]);
    /// The yield's speech has gone still: ask for its words.
    fn finalize(&mut self);
    /// The words, once they came: at most once a yield.
    fn words(&mut self) -> Option<String>;
    /// The yield is over: what it heard is dropped, nothing more of it goes
    /// out once this returns, and its words, if they come later, are not
    /// given.
    fn forget(&mut self);
    /// The reply is over: as `forget`, and no stream is kept.
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
    /// The microphone was closed ([`Self::close`]): the duplex rule's
    /// yield, which a hand Stop or a head touch leaves in force, is not a
    /// new one, and is let be until it ends (the reviewer on #970).
    closed: bool,
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
            closed: false,
        }
    }

    /// One capture block, after the rule has heard it. `replying`: a reply
    /// is under way. `yielded`: a yield is in force after this block, and
    /// `still` its blocks of stillness. `held`: the audio held for a yield
    /// that began at this block, this block's own among it; `samples`: this
    /// block's audio, captured at `captured_at_us`.
    pub fn block(
        &mut self,
        replying: bool,
        yielded: bool,
        still: Option<u8>,
        held: Option<&[crate::activity::ObservedAudio]>,
        captured_at_us: u64,
        samples: &[i16],
    ) -> Tell {
        if self.closed {
            if yielded {
                return Tell::Nothing;
            }
            self.closed = false;
        }
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
                Some(held) => held
                    .iter()
                    .for_each(|block| self.ears.hear(block.captured_at_us, &block.samples)),
                None => self.ears.hear(captured_at_us, samples),
            }
            tell = Tell::Await;
        } else if yielded {
            if !self.asked {
                self.ears.hear(captured_at_us, samples);
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
        self.closed = true;
    }
}

/// The second voice's speech-to-text, on its own thread so that nothing on
/// the coordinator's path waits for the network.
///
/// What may go out is decided apart from the queue to that thread
/// ([`Live`], the reviewer on #970): each yield has a number, every block
/// and every result carries it, and forgetting a yield, resting or
/// dropping the ears changes the live number at once. The thread checks
/// it under its lock as it hands each block to the stream, so once
/// [`Ears::forget`] or [`Ears::rest`] has returned, or the ears are
/// dropped on any way out of the conversation, nothing of that yield is
/// handed on, however much of it is still queued. The thread is woken past
/// the queue and drops the yield's stream, which ends its socket's writer.
/// Words are taken only for the yield in force.
pub struct RelayEars {
    commands: queue::Sender<Command>,
    heard: mpsc::Receiver<(u64, String)>,
    live: Arc<Live>,
    ended: mpsc::Receiver<()>,
}

#[derive(Debug)]
enum Command {
    Ready,
    Hear {
        of: u64,
        captured_at_us: u64,
        samples: Vec<i16>,
    },
    Finalize {
        of: u64,
    },
    Rest,
}

/// How long a failed connection waits before the next.
const RETRY: Duration = Duration::from_secs(5);
/// The most audio kept while no stream is ready: 3 s of 10 ms blocks.
const MOST_WAITING: usize = 300;
/// A block captured longer ago than this is never handed on: the most
/// kept while no stream is ready.
const MOST_AGE_US: u64 = 3_000_000;
/// The most commands queued for the thread: a yield's held audio (up to
/// 1.6 s with `LAMP_FIRST_WORDS=kept`) and its 1.1 s, with room to spare.
/// One that does not fit is dropped: the yield is then unheard.
const MOST_QUEUED: usize = 512;
/// How long dropping the ears waits for the thread to end.
const DROP_WAIT: Duration = Duration::from_millis(250);
/// The live number once the ears are dropped: nothing more goes.
const CLOSED: u64 = u64::MAX;

/// Which yield's audio may still go out and whose words may still be
/// taken, shared by the coordinator and the thread, apart from the queue.
struct Live {
    number: Mutex<u64>,
    revoked: tokio::sync::Notify,
    /// The tests' holds on the thread: before each command, before it
    /// tells words, and whether it has ended.
    #[cfg(test)]
    pause: Mutex<()>,
    #[cfg(test)]
    hold_words: Mutex<()>,
    #[cfg(test)]
    holding_words: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    over: std::sync::atomic::AtomicBool,
}

impl Live {
    fn new() -> Self {
        Self {
            number: Mutex::new(0),
            revoked: tokio::sync::Notify::new(),
            #[cfg(test)]
            pause: Mutex::new(()),
            #[cfg(test)]
            hold_words: Mutex::new(()),
            #[cfg(test)]
            holding_words: std::sync::atomic::AtomicBool::new(false),
            #[cfg(test)]
            over: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn number(&self) -> std::sync::MutexGuard<'_, u64> {
        self.number.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> u64 {
        *self.number()
    }

    /// Nothing of the yield in force goes on, from now: the next has a new
    /// number. The thread is woken to drop what it holds of it.
    fn revoke(&self) {
        {
            let mut number = self.number();
            if *number != CLOSED {
                *number += 1;
            }
        }
        self.revoked.notify_one();
    }

    /// Nothing goes on, ever.
    fn close(&self) {
        *self.number() = CLOSED;
        self.revoked.notify_one();
    }

    /// `samples` of the yield `of`, captured at `captured_at_us`, handed to
    /// `stream` if that yield is in force and the block is not too old:
    /// checked and handed under the lock a revocation takes.
    fn hand(&self, of: u64, captured_at_us: u64, samples: &[i16], stream: &SttStream) -> bool {
        let number = self.number();
        *number == of
            && lamp_ipc::monotonic_us().saturating_sub(captured_at_us) <= MOST_AGE_US
            && stream.send(samples).is_ok()
    }
}

impl RelayEars {
    /// The speech-to-text `config` names, from a thread of its own.
    pub fn start(config: SecondConfig) -> io::Result<Self> {
        Self::with(move || {
            let config = config.clone();
            async move {
                SttStream::connect(&config)
                    .await
                    .map_err(|error| error.to_string())
            }
        })
    }

    /// The same, with streams from `connect`.
    fn with<C, F>(connect: C) -> io::Result<Self>
    where
        C: Fn() -> F + Send + 'static,
        F: Future<Output = Result<SttStream, String>> + Send + 'static,
    {
        let (commands, queued) = queue::channel(MOST_QUEUED);
        let (said, heard) = mpsc::channel();
        let (end, ended) = mpsc::channel();
        let live = Arc::new(Live::new());
        let shared = Arc::clone(&live);
        std::thread::Builder::new()
            .name("lamp-voice-stop-words".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    // Its streams and their tasks go with the runtime.
                    Ok(runtime) => runtime.block_on(serve(connect, queued, said, &shared)),
                    Err(error) => {
                        eprintln!("lamp-voice: stop words cannot be heard: {error}");
                    }
                }
                #[cfg(test)]
                shared.over.store(true, std::sync::atomic::Ordering::SeqCst);
                let _ = end.send(());
            })?;
        Ok(Self {
            commands,
            heard,
            live,
            ended,
        })
    }

    fn send(&self, command: Command) {
        let _ = self.commands.try_send(command);
    }

    fn drain(&self) {
        while self.heard.try_recv().is_ok() {}
    }
}

impl Ears for RelayEars {
    fn ready(&mut self) {
        self.send(Command::Ready);
    }
    fn hear(&mut self, captured_at_us: u64, samples: &[i16]) {
        self.send(Command::Hear {
            of: self.live.now(),
            captured_at_us,
            samples: samples.to_vec(),
        });
    }
    fn finalize(&mut self) {
        self.send(Command::Finalize {
            of: self.live.now(),
        });
    }
    fn words(&mut self) -> Option<String> {
        let live = self.live.now();
        // Words of a yield that is over are not this one's (the
        // reviewer's P2 on #970).
        std::iter::from_fn(|| self.heard.try_recv().ok())
            .find(|(of, _)| *of == live)
            .map(|(_, words)| words)
    }
    fn forget(&mut self) {
        self.live.revoke();
        self.drain();
    }
    fn rest(&mut self) {
        self.live.revoke();
        self.send(Command::Rest);
        self.drain();
    }
}

impl Drop for RelayEars {
    /// On every way out of the conversation, the mute's and the privacy
    /// loss's returns among them: nothing more goes, and the thread is
    /// given a moment to end.
    fn drop(&mut self) {
        self.live.close();
        let _ = self.ended.recv_timeout(DROP_WAIT);
    }
}

/// The thread's loop: a stream connected and kept ready while a reply is
/// under way, the yield's audio to it, and its words back once asked for.
async fn serve<C, F>(
    connect: C,
    mut queued: queue::Receiver<Command>,
    said: mpsc::Sender<(u64, String)>,
    live: &Live,
) where
    C: Fn() -> F,
    F: Future<Output = Result<SttStream, String>> + Send + 'static,
{
    let mut wanted = false;
    let mut ready: Option<SttStream> = None;
    // The stream a yield's audio goes to, and that yield.
    let mut current: Option<(u64, SttStream)> = None;
    let mut waiting: VecDeque<(u64, u64, Vec<i16>)> = VecDeque::new();
    let mut finalize_waiting: Option<u64> = None;
    let mut connecting: Option<tokio::task::JoinHandle<Result<SttStream, String>>> = None;
    let mut finishing: Option<(u64, tokio::task::JoinHandle<Result<String, String>>)> = None;
    let mut retry_at: Option<tokio::time::Instant> = None;
    let mut told_failure = false;
    loop {
        // What a revocation took away is dropped before anything else.
        let number = live.now();
        if number == CLOSED {
            return;
        }
        if current.as_ref().is_some_and(|(of, _)| *of != number) {
            current = None;
        }
        waiting.retain(|(of, _, _)| *of == number);
        finalize_waiting = finalize_waiting.filter(|of| *of == number);
        if finishing.as_ref().is_some_and(|(of, _)| *of != number)
            && let Some((_, task)) = finishing.take()
        {
            task.abort();
        }
        if wanted
            && ready.is_none()
            && connecting.is_none()
            && retry_at.is_none_or(|at| tokio::time::Instant::now() >= at)
        {
            connecting = Some(tokio::spawn(connect()));
        }
        tokio::select! {
            biased;
            () = live.revoked.notified() => {}
            command = queued.recv() => {
                #[cfg(test)]
                drop(live.pause.lock().unwrap_or_else(PoisonError::into_inner));
                match command {
                    None => return,
                    Some(Command::Ready) => wanted = true,
                    Some(Command::Hear { of, captured_at_us, samples }) => {
                        if of != live.now() {
                            continue;
                        }
                        if current.is_none() {
                            current = ready.take().map(|stream| (of, stream));
                        }
                        match &current {
                            Some((_, stream)) => {
                                for (of, at, block) in waiting.drain(..) {
                                    live.hand(of, at, &block, stream);
                                }
                                live.hand(of, captured_at_us, &samples, stream);
                            }
                            None => {
                                if waiting.len() == MOST_WAITING {
                                    waiting.pop_front();
                                }
                                waiting.push_back((of, captured_at_us, samples));
                            }
                        }
                    }
                    Some(Command::Finalize { of }) => {
                        if of != live.now() {
                            continue;
                        }
                        match current.take() {
                            Some((of, stream)) => {
                                finishing = Some((of, tokio::spawn(async move {
                                    stream.finished().await.map_err(|error| error.to_string())
                                })));
                            }
                            None => finalize_waiting = Some(of),
                        }
                    }
                    Some(Command::Rest) => {
                        wanted = false;
                        ready = None;
                        current = None;
                        waiting.clear();
                        finalize_waiting = None;
                        if let Some(task) = connecting.take() {
                            task.abort();
                        }
                        if let Some((_, task)) = finishing.take() {
                            task.abort();
                        }
                    }
                }
            },
            connected = async { connecting.as_mut().expect("guarded").await }, if connecting.is_some() => {
                connecting = None;
                match connected {
                    Ok(Ok(stream)) => {
                        told_failure = false;
                        retry_at = None;
                        let number = live.now();
                        if current.is_none() && (!waiting.is_empty() || finalize_waiting.is_some()) {
                            for (of, at, block) in waiting.drain(..) {
                                live.hand(of, at, &block, &stream);
                            }
                            match finalize_waiting.take() {
                                Some(of) => {
                                    finishing = Some((of, tokio::spawn(async move {
                                        stream.finished().await.map_err(|error| error.to_string())
                                    })));
                                }
                                None => current = Some((number, stream)),
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
            finished = async { (&mut finishing.as_mut().expect("guarded").1).await }, if finishing.is_some() => {
                let Some((of, _)) = finishing.take() else {
                    continue;
                };
                if let Ok(Ok(words)) = finished {
                    #[cfg(test)]
                    let _held = {
                        live.holding_words.store(true, std::sync::atomic::Ordering::SeqCst);
                        live.hold_words.lock().unwrap_or_else(PoisonError::into_inner)
                    };
                    let _ = said.send((of, words));
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
            // Turned round (the reviewer on #970).
            "Don't stop.",
            "No, don't stop.",
            "Don\u{2019}t stop, I love this part.",
            "Never stop.",
        ] {
            assert!(!stop_phrase(not), "{not}");
        }
        // A stop word after one that is turned round still counts.
        assert!(stop_phrase("Don't worry, stop."));
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
        fn hear(&mut self, _: u64, samples: &[i16]) {
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
            words.block(true, false, None, None, 0, &block(1)),
            Tell::Nothing
        );
        let held = [observed(2), observed(3), observed(4)];
        assert_eq!(
            words.block(true, true, Some(0), Some(&held), 0, &block(4)),
            Tell::Await
        );
        assert_eq!(
            words.block(true, true, Some(0), None, 0, &block(5)),
            Tell::Nothing
        );
        for still in 1..=10 {
            assert_eq!(
                words.block(true, true, Some(still), None, 0, &block(6)),
                Tell::Nothing
            );
        }
        script.lock().unwrap().words = Some("Wait, wait.".into());
        assert_eq!(
            words.block(true, true, Some(11), None, 0, &block(7)),
            Tell::Words {
                stop: true,
                count: 2
            }
        );
        script.lock().unwrap().words = Some("Stop.".into());
        assert_eq!(
            words.block(true, true, Some(12), None, 0, &block(7)),
            Tell::Nothing
        );
        assert_eq!(
            words.block(true, false, None, None, 0, &block(8)),
            Tell::Nothing
        );
        assert_eq!(
            words.block(false, false, None, None, 0, &block(9)),
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
        words.block(true, false, None, None, 0, &block(1));
        assert_eq!(
            words.block(true, true, Some(0), None, 0, &block(2)),
            Tell::Await
        );
        assert_eq!(
            words.block(true, false, None, None, 0, &block(3)),
            Tell::Unheard
        );
        assert_eq!(
            words.block(true, true, Some(0), None, 0, &block(4)),
            Tell::Await
        );
        words.close();
        words.close();
        // The reviewer on #970: a hand Stop leaves the duplex rule's yield in
        // force; it is not begun again, and not called unheard when it ends.
        assert_eq!(
            words.block(true, true, Some(3), None, 0, &block(40)),
            Tell::Nothing
        );
        assert_eq!(
            words.block(true, false, None, None, 0, &block(5)),
            Tell::Nothing
        );
        let told = script.lock().unwrap().told.clone();
        assert_eq!(
            told,
            [
                "ready", "hear 2", "forget", "hear 4", "forget", "rest", "ready"
            ]
        );
        // The next yield is heard again.
        assert_eq!(
            words.block(true, true, Some(0), None, 0, &block(6)),
            Tell::Await
        );
    }

    /// A stand-in for the speech-to-text on an in-memory socket: it counts
    /// the audio bytes it is sent, and answers each stream's `Finalize`
    /// with the next of its words.
    struct Service {
        audio: Arc<std::sync::atomic::AtomicUsize>,
        connections: Arc<std::sync::atomic::AtomicUsize>,
        words: Arc<Mutex<VecDeque<&'static str>>>,
    }

    type Connected =
        std::pin::Pin<Box<dyn Future<Output = Result<SttStream, String>> + Send + 'static>>;

    impl Service {
        fn new(words: &[&'static str]) -> Self {
            Self {
                audio: Arc::default(),
                connections: Arc::default(),
                words: Arc::new(Mutex::new(words.iter().copied().collect())),
            }
        }

        fn connect(&self) -> impl Fn() -> Connected + Send + 'static {
            let (audio, connections, words) = (
                Arc::clone(&self.audio),
                Arc::clone(&self.connections),
                Arc::clone(&self.words),
            );
            move || {
                use futures_util::{SinkExt, StreamExt};
                use tokio_tungstenite::{
                    WebSocketStream,
                    tungstenite::{Message, protocol::Role},
                };
                connections.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let audio = Arc::clone(&audio);
                let said = words.lock().unwrap().pop_front().unwrap_or("");
                Box::pin(async move {
                    let (near, far) = tokio::io::duplex(1 << 16);
                    tokio::spawn(async move {
                        let mut socket =
                            WebSocketStream::from_raw_socket(far, Role::Server, None).await;
                        while let Some(Ok(message)) = socket.next().await {
                            match message {
                                Message::Binary(bytes) => {
                                    audio.fetch_add(
                                        bytes.len(),
                                        std::sync::atomic::Ordering::SeqCst,
                                    );
                                }
                                Message::Text(text) if text.contains("Finalize") => {
                                    let results = format!(
                                        r#"{{"type":"Results","is_final":true,"speech_final":true,"from_finalize":true,"start":0.0,"duration":30.0,"channel":{{"alternatives":[{{"transcript":"{said}"}}]}}}}"#
                                    );
                                    let _ = socket.send(Message::text(results)).await;
                                    let _ = socket.close(None).await;
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                    Ok(SttStream::on(
                        WebSocketStream::from_raw_socket(near, Role::Client, None).await,
                    ))
                }) as Connected
            }
        }

        fn audio(&self) -> usize {
            self.audio.load(std::sync::atomic::Ordering::SeqCst)
        }

        fn connections(&self) -> usize {
            self.connections.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    fn until(what: &str, mut done: impl FnMut() -> bool) {
        let began = std::time::Instant::now();
        while !done() {
            assert!(began.elapsed() < Duration::from_secs(5), "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// The reviewer's P1 on #970, through the real thread: a yield's audio
    /// queued behind it while it is held never reaches the service once the
    /// yield is forgotten, the reply rested, or the ears dropped (as every
    /// way out of the conversation drops them, the mute's and the privacy
    /// loss's among them). After a forget, the next yield's audio goes. A
    /// block captured too long ago never goes.
    #[test]
    fn audio_queued_for_a_yield_never_goes_once_it_is_forgotten_rested_or_dropped() {
        for way in ["forget", "rest", "drop"] {
            let service = Service::new(&[]);
            let mut ears = Some(RelayEars::with(service.connect()).unwrap());
            let live = Arc::clone(&ears.as_ref().unwrap().live);
            ears.as_mut().unwrap().ready();
            until("a stream is asked for", || service.connections() == 1);
            let paused = live.pause.lock().unwrap();
            for _ in 0..50 {
                ears.as_mut()
                    .unwrap()
                    .hear(lamp_ipc::monotonic_us(), &[1; 160]);
            }
            match way {
                "forget" => ears.as_mut().unwrap().forget(),
                "rest" => ears.as_mut().unwrap().rest(),
                _ => drop(ears.take()),
            }
            drop(paused);
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(service.audio(), 0, "{way}");
            match ears.as_mut() {
                Some(ears) if way == "forget" => {
                    ears.hear(lamp_ipc::monotonic_us(), &[1; 160]);
                    until("the next yield's audio goes", || service.audio() == 320);
                }
                Some(_) => {}
                None => until("the thread has ended", || {
                    live.over.load(std::sync::atomic::Ordering::SeqCst)
                }),
            }
        }
        // Too old: captured 4 s ago, it never goes; the block after it does.
        let service = Service::new(&[]);
        let mut ears = RelayEars::with(service.connect()).unwrap();
        ears.ready();
        until("a stream is asked for", || service.connections() == 1);
        ears.hear(lamp_ipc::monotonic_us() - 4_000_000, &[1; 160]);
        ears.hear(lamp_ipc::monotonic_us(), &[1; 160]);
        until("the fresh block goes", || service.audio() >= 320);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(service.audio(), 320);
    }

    /// The reviewer's P2 on #970, through the real thread: the words of a
    /// yield that come after it was forgotten, and before the next yield
    /// asks, are never taken for the next yield's; its own are.
    #[test]
    fn words_that_come_for_a_yield_after_it_is_forgotten_are_not_the_next_ones() {
        let service = Service::new(&["Stop.", "Mm-hm."]);
        let mut ears = RelayEars::with(service.connect()).unwrap();
        let live = Arc::clone(&ears.live);
        ears.ready();
        until("a stream is asked for", || service.connections() == 1);
        let held = live.hold_words.lock().unwrap();
        ears.hear(lamp_ipc::monotonic_us(), &[1; 160]);
        ears.finalize();
        until("the first yield's words have come", || {
            live.holding_words.load(std::sync::atomic::Ordering::SeqCst)
        });
        ears.forget();
        // The thread tells them only now, after the yield is over.
        drop(held);
        until("a stream for the next yield", || service.connections() == 2);
        ears.hear(lamp_ipc::monotonic_us(), &[1; 160]);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(ears.words(), None);
        ears.finalize();
        let mut words = None;
        until("the next yield's own words", || {
            words = words.take().or_else(|| ears.words());
            words.is_some()
        });
        assert_eq!(words.as_deref(), Some("Mm-hm."));
    }
}
