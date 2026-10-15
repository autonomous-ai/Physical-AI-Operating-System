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
/// About 6 s of 20 ms frames waiting to be written.
const QUEUE: usize = 300;
const MAX_MESSAGE: usize = 1 << 20;

/// V1's query for nova models, in its order.
fn query(language: Option<&str>) -> [(&'static str, &str); 9] {
    [
        ("model", MODEL),
        ("encoding", "linear16"),
        ("sample_rate", "16000"),
        ("channels", "1"),
        ("language", language.unwrap_or("en")),
        ("smart_format", "true"),
        ("interim_results", "true"),
        ("endpointing", "1500"),
        ("vad_events", "true"),
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

/// One `Results` message: the best alternative's words.
#[derive(Debug, PartialEq)]
pub(crate) struct Heard {
    pub transcript: String,
    pub is_final: bool,
    pub speech_final: bool,
}

/// `None` for any other message (metadata, `SpeechStarted`,
/// `UtteranceEnd`) and for results with no words.
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
    let transcript = results
        .channel?
        .alternatives
        .into_iter()
        .next()?
        .transcript
        .trim()
        .to_owned();
    (!transcript.is_empty()).then_some(Heard {
        transcript,
        is_final: results.is_final,
        speech_final: results.speech_final,
    })
}

/// The final segments in the order heard, joined by spaces. Interim
/// results are previews of a segment and are dropped.
#[derive(Default)]
pub(crate) struct Transcript(Vec<String>);

impl Transcript {
    pub fn add(&mut self, heard: Heard) {
        if heard.is_final {
            self.0.push(heard.transcript);
        }
    }
    pub fn text(&self) -> String {
        self.0.join(" ")
    }
}

/// One utterance's stream. The socket is owned by a task that writes the
/// queued audio, keeps an idle socket alive and collects the transcript.
pub struct SttStream {
    /// `None` once finishing: the task then sends `CloseStream`.
    audio: Option<mpsc::Sender<Vec<u8>>>,
    task: JoinHandle<Result<String>>,
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
        let (audio, queued) = mpsc::channel(QUEUE);
        Ok(Self {
            audio: Some(audio),
            task: tokio::spawn(run(socket, queued)),
        })
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
    /// Dropping the stream instead abandons it at once.
    pub async fn finish(mut self) -> Result<String> {
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
) -> Result<String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = socket.split();
    let mut transcript = Transcript::default();
    let mut last_sent = Instant::now();
    let mut closing = false;
    loop {
        tokio::select! {
            frame = queued.recv(), if !closing => {
                let message = match frame {
                    Some(bytes) => Message::binary(bytes),
                    None => {
                        closing = true;
                        Message::text(r#"{"type":"CloseStream"}"#)
                    }
                };
                write.send(message).await.map_err(|_| Error::Transport("speech-to-text send"))?;
                last_sent = Instant::now();
            }
            _ = tokio::time::sleep_until(last_sent + KEEP_ALIVE), if !closing => {
                write
                    .send(Message::text(r#"{"type":"KeepAlive"}"#))
                    .await
                    .map_err(|_| Error::Transport("speech-to-text send"))?;
                last_sent = Instant::now();
            }
            message = read.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Some(words) = heard(&text) {
                        transcript.add(words);
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return if closing {
                        Ok(transcript.text())
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
        &language=en&smart_format=true&interim_results=true&endpointing=1500&vad_events=true";

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
        assert_eq!(stream.finish().await.unwrap(), "hello lamp");
        assert_eq!(server.await.unwrap(), 8);
    }
}
