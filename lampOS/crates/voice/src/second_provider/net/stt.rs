//! Streaming speech-to-text over Deepgram's websocket protocol, straight to
//! Deepgram or through the Autonomous proxy, as V1's
//! `hal/drivers/voice/stt/{deepgram,autonomous}.py` do with `nova-3-general`.
use super::{CONNECT, Error, Result, openai_base, tls};
use crate::second_config::{SecondConfig, Stt};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::{sync::Arc, time::Duration};
use tokio::{sync::mpsc, task::JoinHandle, time::Instant};
use tokio_tungstenite::{
    Connector, connect_async_tls_with_config,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};

const MODEL: &str = "nova-3-general";
const DEEPGRAM: &str = "wss://api.deepgram.com/v1/listen";
/// Deepgram closes a socket that hears nothing for 10 s.
const KEEP_ALIVE: Duration = Duration::from_secs(5);
/// V1 waits as long for the final words after `CloseStream`.
const FINISH: Duration = Duration::from_secs(15);
/// How long the end of the line waits for `Finalize`'s final words before
/// falling back to `CloseStream` (#779): on lamp-52e6 `CloseStream`'s took
/// 0.56 to 0.66 s, so a proxy that passes `Finalize` answers well within.
const FINALIZE: Duration = Duration::from_secs(1);
/// About 6 s of 20 ms frames waiting to be written.
const QUEUE: usize = 300;
const MAX_MESSAGE: usize = 1 << 20;

/// V1's query for nova models, in its order, with the lamp's name as a
/// key term as V1 boosts it: without it, on lamp-52e6 on 2026-10-15, "Hey
/// Lamp" came back as "Palin", "Palan" and "Len", and the model took the
/// question for other people's talk.
// ponytail: the name is the character's own ("Lamp"); a lamp renamed
// would pass its name here.
fn query(language: Option<&str>) -> [(&'static str, &str); 10] {
    [
        ("model", MODEL),
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("language", language.unwrap_or("en")),
        ("smart_format", "true"),
        ("interim_results", "true"),
        // V1 waited for 1.5 s of silence; the coordinator ends a line
        // after 0.4 to 1.5 s, so Deepgram's `speech_final` came after the
        // End it could have spared (#779). At 300 ms it is usually there.
        ("endpointing", "300"),
        ("vad_events", "true"),
        ("keyterm", "Lamp"),
    ]
}

/// The websocket URL: the Autonomous proxy at `proxy`, else Deepgram.
pub(crate) fn url(proxy: Option<&str>, language: Option<&str>) -> Result<reqwest::Url> {
    let base = match proxy {
        None => DEEPGRAM.to_owned(),
        Some(base_url) => {
            let base = openai_base(base_url)
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1);
            format!("{base}/ws/audio/transcriptions")
        }
    };
    let url = reqwest::Url::parse_with_params(&base, query(language)).map_err(|_| Error::Url)?;
    match url.scheme() {
        "wss" => Ok(url),
        "ws" if cfg!(test) => Ok(url),
        _ => Err(Error::Url),
    }
}

/// Audio seconds a final segment may end short of the audio sent and still
/// cover it: a 20 ms frame and a little rounding.
const COVERED: f64 = 0.05;

/// One `Results` message: the best alternative's words (empty for
/// silence), and where in the audio the segment ends, in seconds.
#[derive(Debug, PartialEq)]
pub(crate) struct Heard {
    pub transcript: String,
    pub is_final: bool,
    pub speech_final: bool,
    /// The answer to a `Finalize`: the words it asked for are all here.
    pub from_finalize: bool,
    pub end: f64,
}

/// `None` for any other message (metadata, `SpeechStarted`,
/// `UtteranceEnd`).
pub(crate) fn heard(text: &str) -> Option<Heard> {
    #[derive(Deserialize)]
    struct Results {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        channel: Option<Channel>,
        #[serde(default)]
        is_final: bool,
        #[serde(default)]
        speech_final: bool,
        #[serde(default)]
        from_finalize: bool,
        #[serde(default)]
        start: f64,
        #[serde(default)]
        duration: f64,
    }
    #[derive(Deserialize)]
    struct Channel {
        alternatives: Vec<Alternative>,
    }
    #[derive(Deserialize)]
    struct Alternative {
        transcript: String,
    }
    let results: Results = serde_json::from_str(text).ok()?;
    if results.kind != "Results" {
        return None;
    }
    let end = results.start + results.duration;
    let transcript = results
        .channel?
        .alternatives
        .into_iter()
        .next()?
        .transcript
        .trim()
        .to_owned();
    Some(Heard {
        transcript,
        is_final: results.is_final,
        speech_final: results.speech_final,
        from_finalize: results.from_finalize,
        end,
    })
}

/// For a `SpeechStarted` message (someone is talking again), where in the
/// audio the speech began, in seconds.
pub(crate) fn speech_started(text: &str) -> Option<f64> {
    let message = serde_json::from_str::<serde_json::Value>(text).ok()?;
    (message["type"] == "SpeechStarted").then(|| message["timestamp"].as_f64().unwrap_or(0.0))
}

/// What was heard since the last final, and where in the audio, in seconds.
#[derive(Clone, Copy)]
enum Pending {
    /// An interim result with words, ending there.
    Interim(f64),
    /// A `SpeechStarted`, starting there.
    SpeechStarted(f64),
}

/// The final segments in the order heard, joined by spaces. Interim
/// results are previews of a segment and are dropped.
#[derive(Default)]
pub(crate) struct Transcript {
    segments: Vec<String>,
    /// Where the last final segment ends in the audio, in seconds; `None`
    /// before any.
    covered: Option<f64>,
    /// The last final segment closed an utterance (`speech_final`).
    ended: bool,
    /// Words (an interim result or `SpeechStarted`) since the last final.
    pending: Option<Pending>,
}

impl Transcript {
    pub fn add(&mut self, heard: Heard) {
        if !heard.is_final {
            if !heard.transcript.is_empty() {
                self.pending = Some(Pending::Interim(heard.end));
            }
            return;
        }
        self.covered = Some(heard.end);
        self.ended = heard.speech_final;
        self.pending = None;
        if !heard.transcript.is_empty() {
            self.segments.push(heard.transcript);
        }
    }
    pub fn speech_started(&mut self, at: f64) {
        self.pending = Some(Pending::SpeechStarted(at));
    }
    /// Nothing more is coming for the `sent` seconds of audio: final
    /// segments cover all of it, and nobody has been heard since. Then the
    /// words need not wait for `Finalize` or `CloseStream`. An earlier
    /// `speech_final` is only a pause: audio sent after it may still have
    /// words on their way (#878).
    pub fn settled(&self, sent: f64) -> bool {
        self.pending.is_none()
            && self
                .covered
                .is_some_and(|covered| covered + COVERED >= sent)
    }
    /// What [`Self::settled`] decided on for `sent` seconds, in ms and
    /// without words: the journal's account of the end of the line (#779).
    fn at_end(&self, sent: f64) -> String {
        let ms = |seconds: f64| (seconds * 1000.0).round() as i64;
        let covered = self
            .covered
            .map_or("none".to_owned(), |covered| ms(covered).to_string());
        let pending = match self.pending {
            None => "none".to_owned(),
            Some(Pending::Interim(at)) => format!(
                "interim {} ms after the last final",
                ms(at - self.covered.unwrap_or(0.0))
            ),
            Some(Pending::SpeechStarted(at)) => format!(
                "speech_started {} ms after the last final",
                ms(at - self.covered.unwrap_or(0.0))
            ),
        };
        format!(
            "settled={}, speech_final={}, finals cover {covered} of {} ms sent, pending={pending}",
            self.settled(sent),
            self.ended,
            ms(sent)
        )
    }
    pub fn text(&self) -> String {
        self.segments.join(" ")
    }
}

/// One utterance's stream. The socket is owned by a task that writes the
/// queued audio, keeps an idle socket alive and collects the transcript.
pub struct SttStream {
    /// `None` once finishing: the task then sends `CloseStream`.
    audio: Option<mpsc::Sender<Vec<u8>>>,
    /// The words, and the account of the end of the line once it came.
    task: JoinHandle<Result<(String, Option<String>)>>,
}

impl SttStream {
    pub async fn connect(config: &SecondConfig) -> Result<Self> {
        let (proxy, key) = match &config.stt {
            Stt::Deepgram { key } => (None, key),
            Stt::Autonomous { base_url, key } => (Some(base_url.as_str()), key),
        };
        Self::open(url(proxy, config.stt_language.as_deref())?, key.expose()).await
    }

    async fn open(url: reqwest::Url, key: &str) -> Result<Self> {
        let mut request = url.as_str().into_client_request().map_err(|_| Error::Url)?;
        let mut auth =
            HeaderValue::from_str(&format!("Token {key}")).map_err(|_| Error::Protocol("key"))?;
        auth.set_sensitive(true);
        request.headers_mut().insert("authorization", auth);
        let socket_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE))
            .max_frame_size(Some(MAX_MESSAGE));
        let (socket, _) = tokio::time::timeout(
            CONNECT,
            connect_async_tls_with_config(
                request,
                Some(socket_config),
                true,
                Some(Connector::Rustls(Arc::new(tls()?))),
            ),
        )
        .await
        .map_err(|_| Error::Timeout("speech-to-text connect"))?
        .map_err(|error| match error {
            tokio_tungstenite::tungstenite::Error::Http(response) => {
                Error::Status(response.status().as_u16())
            }
            _ => Error::Transport("speech-to-text connect"),
        })?;
        Ok(Self::start(socket))
    }

    fn start<S>(socket: tokio_tungstenite::WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Send + Unpin + 'static,
    {
        let (audio, queued) = mpsc::channel(QUEUE);
        Self {
            audio: Some(audio),
            task: tokio::spawn(run(socket, queued)),
        }
    }

    /// Queue 16 kHz mono samples. Fails when the socket has gone or the
    /// queue is full (the service stopped reading); nothing waits.
    pub fn send(&self, samples: &[i16]) -> Result<()> {
        let bytes = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        let audio = self
            .audio
            .as_ref()
            .ok_or(Error::Transport("speech-to-text stream"))?;
        audio.try_send(bytes).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => Error::Protocol("speech-to-text backlog"),
            mpsc::error::TrySendError::Closed(_) => Error::Transport("speech-to-text stream"),
        })
    }

    /// Ask for the last words and wait, bounded, for the service to close.
    /// The final segments, joined; empty when nothing was heard.
    /// Dropping the stream instead abandons it at once. Journals, for
    /// `request`, what the words' state was when the line ended.
    pub async fn finish(self, request: u64) -> Result<String> {
        let (words, at_end) = self.finish_told().await?;
        if let Some(at_end) = at_end {
            eprintln!("lamp-voice second: request {request} at the line's end: {at_end}");
        }
        Ok(words)
    }

    /// The words, and the journal's account of the end of the line.
    async fn finish_told(mut self) -> Result<(String, Option<String>)> {
        self.audio = None;
        match tokio::time::timeout(FINISH, &mut self.task).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::Transport("speech-to-text stream")),
            Err(_) => Err(Error::Timeout("speech-to-text final transcript")),
        }
    }
}

impl Drop for SttStream {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    mut queued: mpsc::Receiver<Vec<u8>>,
) -> Result<(String, Option<String>)>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = socket.split();
    let mut transcript = Transcript::default();
    let mut last_sent = Instant::now();
    let mut samples_sent = 0usize;
    // Set at the end of the line, when `Finalize` went: until when its
    // final words are waited for.
    let mut finalize_by: Option<Instant> = None;
    // `CloseStream` went: its final words and close are waited for.
    let mut closing = false;
    let mut last_final: Option<Instant> = None;
    let mut at_end = String::new();
    let told = |at_end: &str, from: &str| Some(format!("{at_end}, words from={from}"));
    let close_stream = || Message::text(r#"{"type":"CloseStream"}"#);
    loop {
        let sent = samples_sent as f64 / 16_000.0;
        tokio::select! {
            frame = queued.recv(), if finalize_by.is_none() && !closing => {
                if let Some(bytes) = frame {
                    samples_sent += bytes.len() / 2;
                    write
                        .send(Message::binary(bytes))
                        .await
                        .map_err(|_| Error::Transport("speech-to-text send"))?;
                    last_sent = Instant::now();
                    continue;
                }
                // The end of the line. Words already settled are the
                // answer; otherwise `Finalize` asks for the final at once
                // (#779), and `CloseStream` only if it does not come.
                let before = last_final.map_or("none".to_owned(), |at| {
                    format!("{} ms before End", at.elapsed().as_millis())
                });
                at_end = format!("{}, last final {before}", transcript.at_end(sent));
                if transcript.settled(sent) {
                    let _ = write.send(close_stream()).await;
                    return Ok((transcript.text(), told(&at_end, "settled")));
                }
                write
                    .send(Message::text(r#"{"type":"Finalize"}"#))
                    .await
                    .map_err(|_| Error::Transport("speech-to-text send"))?;
                finalize_by = Some(Instant::now() + FINALIZE);
            }
            _ = tokio::time::sleep_until(last_sent + KEEP_ALIVE), if finalize_by.is_none() && !closing => {
                write
                    .send(Message::text(r#"{"type":"KeepAlive"}"#))
                    .await
                    .map_err(|_| Error::Transport("speech-to-text send"))?;
                last_sent = Instant::now();
            }
            _ = tokio::time::sleep_until(finalize_by.unwrap_or(last_sent)), if finalize_by.is_some() && !closing => {
                // `Finalize` went unanswered: the proxy may not pass it.
                closing = true;
                write
                    .send(close_stream())
                    .await
                    .map_err(|_| Error::Transport("speech-to-text send"))?;
            }
            message = read.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(words) = heard(&text) {
                        let finalized = words.from_finalize;
                        if words.is_final && finalize_by.is_none() {
                            last_final = Some(Instant::now());
                        }
                        transcript.add(words);
                        if finalize_by.is_some() && !closing {
                            if transcript.settled(sent) {
                                // Closes the socket; its answer is not waited for.
                                let _ = write.send(close_stream()).await;
                                return Ok((transcript.text(), told(&at_end, "finalize")));
                            }
                            if finalized {
                                // `Finalize`'s answer short of the audio sent:
                                // audio sent faster than speech (a held
                                // burst) was not all heard yet, and nothing
                                // more came for it on lamp-52e6 ("Hang" for
                                // "Hang on.", #828). `CloseStream` flushes
                                // the rest and closes; its words are waited
                                // for.
                                closing = true;
                                write
                                    .send(close_stream())
                                    .await
                                    .map_err(|_| Error::Transport("speech-to-text send"))?;
                            }
                        }
                    } else if let Some(at) = speech_started(&text) {
                        transcript.speech_started(at);
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return if finalize_by.is_some() {
                        Ok((transcript.text(), told(&at_end, "close_stream")))
                    } else {
                        Err(Error::Transport("speech-to-text stream"))
                    };
                }
                Some(Ok(_)) => {}
                Some(Err(_)) => return Err(Error::Transport("speech-to-text stream")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUERY: &str = "model=nova-3-general&encoding=linear16&sample_rate=16000&channels=1\
        &language=en&smart_format=true&interim_results=true&endpointing=300&vad_events=true\
        &keyterm=Lamp";

    #[test]
    fn the_proxy_url_is_v1s() {
        let url = url(Some("https://campaign-api.autonomous.ai/api/v1/ai/"), None).unwrap();
        assert_eq!(
            url.as_str(),
            format!(
                "wss://campaign-api.autonomous.ai/api/v1/ai/v1/ws/audio/transcriptions?{QUERY}"
            )
        );
    }

    #[test]
    fn deepgram_is_called_directly_in_the_language_asked() {
        assert_eq!(
            url(None, Some("vi")).unwrap().as_str(),
            format!(
                "wss://api.deepgram.com/v1/listen?{}",
                QUERY.replace("language=en", "language=vi")
            )
        );
    }

    #[test]
    fn a_service_that_is_not_a_websocket_is_refused() {
        assert_eq!(
            url(Some("ftp://example.com"), None).unwrap_err(),
            Error::Url
        );
    }

    #[test]
    fn interim_results_are_dropped_and_finals_joined_in_order() {
        let recorded = [
            r#"{"type":"Metadata","request_id":"x"}"#,
            r#"{"type":"SpeechStarted","timestamp":0.1}"#,
            r#"{"type":"Results","is_final":false,"speech_final":false,"channel":{"alternatives":[{"transcript":"what","confidence":0.9}]}}"#,
            r#"{"type":"Results","is_final":true,"speech_final":false,"channel":{"alternatives":[{"transcript":"What time","confidence":0.9}]}}"#,
            r#"{"type":"Results","is_final":true,"speech_final":false,"channel":{"alternatives":[{"transcript":"","confidence":0}]}}"#,
            r#"{"type":"Results","is_final":false,"speech_final":false,"channel":{"alternatives":[{"transcript":"is","confidence":0.9}]}}"#,
            r#"{"type":"Results","is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":" is it? ","confidence":0.9}]}}"#,
            r#"{"type":"UtteranceEnd","last_word_end":2.1}"#,
            "not json",
        ];
        let mut transcript = Transcript::default();
        let mut ends = 0;
        for text in recorded {
            if let Some(words) = heard(text) {
                ends += usize::from(words.speech_final);
                transcript.add(words);
            }
        }
        assert_eq!(transcript.text(), "What time is it?");
        assert_eq!(ends, 1);
    }

    #[test]
    fn words_settle_only_on_full_cover_and_unsettle_on_new_speech() {
        let result = |text: &str, is_final, speech_final, end| Heard {
            transcript: text.to_owned(),
            is_final,
            speech_final,
            from_finalize: false,
            end,
        };
        let mut transcript = Transcript::default();
        assert!(!transcript.settled(1.0), "nothing final yet");
        assert!(!transcript.settled(0.0), "nor for no audio");
        transcript.add(result("what time", true, false, 0.8));
        assert!(!transcript.settled(1.0), "0.2 s not yet covered");
        assert!(transcript.settled(0.8));
        transcript.add(result("is", false, false, 1.0));
        assert!(!transcript.settled(0.8), "an interim with words is pending");
        transcript.add(result("is it", true, true, 1.0));
        assert!(
            !transcript.settled(2.5),
            "an utterance end leaves 1.5 s uncovered"
        );
        assert!(transcript.settled(1.0));
        transcript.add(result("", true, false, 2.5));
        assert!(transcript.settled(2.5), "a silent final covers the rest");
        transcript.add(result("", false, false, 2.5));
        assert!(transcript.settled(2.5), "an empty interim is silence");
        transcript.speech_started(2.6);
        assert!(!transcript.settled(2.5), "someone talks again");
        assert_eq!(transcript.text(), "what time is it");
    }

    #[test]
    fn the_end_of_the_line_is_told_in_ms_without_words() {
        let mut transcript = Transcript::default();
        assert_eq!(
            transcript.at_end(1.5),
            "settled=false, speech_final=false, finals cover none of 1500 ms sent, pending=none"
        );
        transcript.add(Heard {
            transcript: "secret words".to_owned(),
            is_final: true,
            speech_final: true,
            from_finalize: false,
            end: 1.23,
        });
        assert_eq!(
            speech_started(r#"{"type":"SpeechStarted","timestamp":1.35}"#),
            Some(1.35)
        );
        transcript.speech_started(1.35);
        let told = transcript.at_end(1.5);
        assert_eq!(
            told,
            "settled=false, speech_final=true, finals cover 1230 of 1500 ms sent, \
             pending=speech_started 120 ms after the last final"
        );
        assert!(!told.contains("secret"));
    }

    /// Speech to text in memory, on the paused clock: the server reads
    /// audio, sends `before` once some arrived, answers `Finalize` at once
    /// with `finalized`, 50 ms apart (none: ignores it, as a proxy might),
    /// and answers `CloseStream` `round_trip` later with a last final and
    /// its close. The client sends one second of audio, waits for its
    /// results, then sends `more` frames and finishes. Returns the words,
    /// the journal's account and how long `finish` took.
    async fn finish_after(
        before: &'static str,
        finalized: &'static [&'static str],
        round_trip: Duration,
        more: usize,
    ) -> (String, String, Duration) {
        use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Role};
        let (near, far) = tokio::io::duplex(1 << 16);
        let server = tokio::spawn(async move {
            let mut socket = WebSocketStream::from_raw_socket(far, Role::Server, None).await;
            let mut told = false;
            while let Some(Ok(message)) = socket.next().await {
                match message {
                    Message::Binary(_) if !told => {
                        told = true;
                        socket.send(Message::text(before)).await.unwrap();
                    }
                    Message::Text(text) if text.contains("Finalize") => {
                        for finalized in finalized {
                            socket.send(Message::text(*finalized)).await.unwrap();
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                    }
                    Message::Text(text) if text.contains("CloseStream") => break,
                    _ => {}
                }
            }
            tokio::time::sleep(round_trip).await;
            let last = r#"{"type":"Results","is_final":true,"speech_final":true,"start":0.5,"duration":0.5,"channel":{"alternatives":[{"transcript":"from close"}]}}"#;
            let _ = socket.send(Message::text(last)).await;
            let _ = socket.close(None).await;
        });
        let stream =
            SttStream::start(WebSocketStream::from_raw_socket(near, Role::Client, None).await);
        // One second of audio, then its results have time to arrive.
        for _ in 0..50 {
            stream.send(&[0; 320]).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        for _ in 0..more {
            stream.send(&[0; 320]).unwrap();
        }
        let began = Instant::now();
        let (words, at_end) = stream.finish_told().await.unwrap();
        let took = began.elapsed();
        server.abort();
        (words, at_end.unwrap(), took)
    }

    const INTERIM: &str = r#"{"type":"Results","is_final":false,"speech_final":false,"start":0.0,"duration":0.6,"channel":{"alternatives":[{"transcript":"hello"}]}}"#;

    #[tokio::test(start_paused = true)]
    async fn finals_covering_all_the_audio_sent_do_not_wait_for_close_stream() {
        let ended = r#"{"type":"Results","is_final":true,"speech_final":true,"start":0.0,"duration":1.0,"channel":{"alternatives":[{"transcript":"hello lamp"}]}}"#;
        let (words, at_end, took) = finish_after(ended, &[], Duration::from_millis(800), 0).await;
        assert_eq!(words, "hello lamp");
        assert_eq!(took, Duration::ZERO);
        assert!(at_end.ends_with(", words from=settled"), "{at_end}");
    }

    #[tokio::test(start_paused = true)]
    async fn words_still_coming_take_finalizes_answer_without_waiting_for_close_stream() {
        const FINALIZED: &str = r#"{"type":"Results","is_final":true,"speech_final":true,"from_finalize":true,"start":0.0,"duration":1.0,"channel":{"alternatives":[{"transcript":"hello lamp"}]}}"#;
        let (words, at_end, took) =
            finish_after(INTERIM, &[FINALIZED], Duration::from_millis(800), 0).await;
        assert_eq!(words, "hello lamp");
        assert_eq!(took, Duration::ZERO);
        assert!(at_end.ends_with(", words from=finalize"), "{at_end}");
        assert!(!at_end.contains("hello"));
    }

    #[tokio::test(start_paused = true)]
    async fn a_proxy_that_ignores_finalize_falls_back_to_close_stream() {
        let (words, at_end, took) = finish_after(INTERIM, &[], Duration::from_millis(800), 0).await;
        assert_eq!(words, "from close");
        assert_eq!(took, FINALIZE + Duration::from_millis(800));
        assert!(at_end.ends_with(", words from=close_stream"), "{at_end}");
    }

    const SET_A_TIMER: &str = r#"{"type":"Results","is_final":true,"speech_final":true,"start":0.0,"duration":1.0,"channel":{"alternatives":[{"transcript":"set a timer"}]}}"#;
    const THE_REST: &str = r#"{"type":"Results","is_final":true,"speech_final":true,"from_finalize":true,"start":1.0,"duration":1.0,"channel":{"alternatives":[{"transcript":"for twenty minutes"}]}}"#;

    /// An utterance ended at 1 s, then another second was sent whose
    /// results had not come by the end of the line: they are waited for.
    #[tokio::test(start_paused = true)]
    async fn an_utterance_ended_before_more_audio_was_sent_does_not_end_the_line() {
        let (words, at_end, _) =
            finish_after(SET_A_TIMER, &[THE_REST], Duration::from_millis(800), 50).await;
        assert_eq!(words, "set a timer for twenty minutes");
        assert!(at_end.ends_with(", words from=finalize"), "{at_end}");
    }

    /// After `Finalize`, an older utterance's final arrives before the
    /// flush's answer: the flush is still waited for.
    #[tokio::test(start_paused = true)]
    async fn an_older_final_after_finalize_does_not_end_the_line() {
        let (words, at_end, took) = finish_after(
            INTERIM,
            &[SET_A_TIMER, THE_REST],
            Duration::from_millis(800),
            50,
        )
        .await;
        assert_eq!(words, "set a timer for twenty minutes");
        assert_eq!(took, Duration::from_millis(50));
        assert!(at_end.ends_with(", words from=finalize"), "{at_end}");
    }

    /// #828, lamp-52e6: after a held burst, `Finalize`'s answer covered
    /// only part of the audio sent ("Hang" for "Hang on.", 0.30 of
    /// 0.59 s). The line does not end on it: `CloseStream` goes at once,
    /// without waiting out `FINALIZE`, and the rest of the words are kept.
    #[tokio::test(start_paused = true)]
    async fn a_finalize_answer_short_of_the_audio_sent_flushes_the_rest() {
        const SHORT: &str = r#"{"type":"Results","is_final":true,"speech_final":true,"from_finalize":true,"start":0.0,"duration":0.3,"channel":{"alternatives":[{"transcript":"hang"}]}}"#;
        let (words, at_end, took) =
            finish_after(INTERIM, &[SHORT], Duration::from_millis(800), 0).await;
        assert_eq!(words, "hang from close");
        // The server's 50 ms after its answer, then `CloseStream`'s round
        // trip: `FINALIZE` is not waited out.
        assert_eq!(took, Duration::from_millis(850));
        assert!(at_end.ends_with(", words from=close_stream"), "{at_end}");
    }

    /// A local plain websocket server stands in for Deepgram.
    // The large error is tungstenite's own handshake callback signature.
    #[allow(clippy::result_large_err)]
    #[tokio::test]
    async fn audio_goes_out_and_the_final_words_come_back() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                |request: &tokio_tungstenite::tungstenite::handshake::server::Request, response| {
                    assert_eq!(request.headers()["authorization"], "Token secret");
                    Ok(response)
                },
            )
            .await
            .unwrap();
            let mut bytes = 0;
            while let Some(Ok(message)) = socket.next().await {
                match message {
                    Message::Binary(audio) => bytes += audio.len(),
                    Message::Text(text) if text.contains("CloseStream") => break,
                    _ => {}
                }
            }
            let result = r#"{"type":"Results","is_final":true,"speech_final":true,"channel":{"alternatives":[{"transcript":"hello lamp"}]}}"#;
            socket.send(Message::text(result)).await.unwrap();
            socket.close(None).await.unwrap();
            bytes
        });
        let url = reqwest::Url::parse(&format!("ws://{address}/v1/listen")).unwrap();
        let stream = SttStream::open(url, "secret").await.unwrap();
        stream.send(&[1, -1, 3]).unwrap();
        stream.send(&[7]).unwrap();
        assert_eq!(stream.finish(1).await.unwrap(), "hello lamp");
        assert_eq!(server.await.unwrap(), 8);
    }
}
