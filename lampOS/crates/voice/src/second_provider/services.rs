//! The seam between the engine and the network: the three services as the
//! engine uses them, with [`Net`] wrapping the real clients.
use super::net::{self, Chat, ChatEvent, Message, SttStream, Tts};
use crate::second_config::SecondConfig;
use futures_util::{
    StreamExt, TryStreamExt,
    future::BoxFuture,
    stream::{self, BoxStream},
};
use std::sync::Arc;

/// Speech-to-text, a chat model and text-to-speech.
pub trait Services: Send + Sync + 'static {
    /// Open speech-to-text for one utterance.
    fn listen(&self) -> BoxFuture<'static, net::Result<Box<dyn Listener>>>;
    /// The answer to `messages`, as it is written; a failure to connect is
    /// the stream's first item.
    fn chat(&self, messages: Vec<Message>) -> BoxStream<'static, net::Result<ChatEvent>>;
    /// `text` spoken, as 24 kHz mono packets; dropping it stops the download.
    fn speak(&self, text: String) -> BoxStream<'static, net::Result<Vec<i16>>>;
}

/// One open utterance.
pub trait Listener: Send {
    /// Queue 16 kHz mono samples; nothing waits.
    fn send(&self, samples: &[i16]) -> net::Result<()>;
    /// The final words; empty when nothing was heard.
    /// `request` names the turn in the journal.
    fn finish(self: Box<Self>, request: u64) -> BoxFuture<'static, net::Result<String>>;
}

impl Listener for SttStream {
    fn send(&self, samples: &[i16]) -> net::Result<()> {
        SttStream::send(self, samples)
    }
    fn finish(self: Box<Self>, request: u64) -> BoxFuture<'static, net::Result<String>> {
        Box::pin(SttStream::finish(*self, request))
    }
}

/// The real services, from the route's configuration.
pub struct Net {
    config: Arc<SecondConfig>,
    chat: Arc<Chat>,
    tts: Arc<Tts>,
}

impl Net {
    pub fn new(config: SecondConfig, tools: &[lamp_gemini::Tool]) -> net::Result<Self> {
        Ok(Self {
            chat: Arc::new(Chat::new(&config, tools)?),
            tts: Arc::new(Tts::new(&config)?),
            config: Arc::new(config),
        })
    }
}

impl Services for Net {
    fn listen(&self) -> BoxFuture<'static, net::Result<Box<dyn Listener>>> {
        let config = self.config.clone();
        Box::pin(async move {
            let stream = SttStream::connect(&config).await?;
            Ok(Box::new(stream) as Box<dyn Listener>)
        })
    }

    fn chat(&self, messages: Vec<Message>) -> BoxStream<'static, net::Result<ChatEvent>> {
        let chat = self.chat.clone();
        stream::once(async move { chat.send(&messages).await })
            .map_ok(|answer| {
                stream::try_unfold(answer, |mut answer| async move {
                    Ok(answer.next().await?.map(|event| (event, answer)))
                })
            })
            .try_flatten()
            .boxed()
    }

    fn speak(&self, text: String) -> BoxStream<'static, net::Result<Vec<i16>>> {
        let tts = self.tts.clone();
        stream::once(async move { tts.speak(&text).await })
            .map_ok(|speech| {
                stream::try_unfold(speech, |mut speech| async move {
                    Ok(speech.next().await?.map(|packet| (packet, speech)))
                })
            })
            .try_flatten()
            .boxed()
    }
}
