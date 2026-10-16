//! A streamed OpenAI-compatible chat completion with function tools: text
//! as it is written, then the tool calls the model asked for. The caller
//! runs the tools and sends their results back as `tool` messages.
use super::{Error, Result, chunk, http_client, openai_base, post};
use crate::second_config::SecondConfig;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Arc;

/// One message of the conversation, in the wire's own shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Message {
    pub role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    fn new(role: &'static str, content: Option<String>) -> Self {
        Self {
            role,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
    pub fn system(text: impl Into<String>) -> Self {
        Self::new("system", Some(text.into()))
    }
    pub fn user(text: impl Into<String>) -> Self {
        Self::new("user", Some(text.into()))
    }
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new("assistant", Some(text.into()))
    }
    /// The model's turn that asked for these calls, with what it said first.
    pub fn assistant_calls(text: Option<String>, calls: &[ToolCall]) -> Self {
        let mut message = Self::new("assistant", text.filter(|t| !t.is_empty()));
        message.tool_calls = calls
            .iter()
            .map(|call| {
                json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.args.to_string()},
                })
            })
            .collect();
        message
    }
    /// One call's result.
    pub fn tool(call_id: impl Into<String>, result: impl Into<String>) -> Self {
        let mut message = Self::new("tool", Some(result.into()));
        message.tool_call_id = Some(call_id.into());
        message
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments object; `{}` when the model sent none.
    pub args: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    Text(String),
    /// Every call of the turn, once the model has finished asking.
    ToolCalls(Vec<ToolCall>),
    Done,
}

/// The lamp's tools as OpenAI function declarations.
fn functions(tools: &[lamp_gemini::Tool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            let parameters = tool
                .parameters
                .clone()
                .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": parameters,
                },
            })
        })
        .collect()
}

/// The lamp's tools as they stand now: asked at each chat request, as a
/// Live session is told them when it opens (`thread::Recalling::equipped`),
/// so the executive's come and go with the executive.
pub type Declare = Arc<dyn Fn() -> Vec<lamp_gemini::Tool> + Send + Sync>;

pub struct Chat {
    client: reqwest::Client,
    url: String,
    auth: String,
    model: String,
    declare: Declare,
}

impl Chat {
    pub fn new(config: &SecondConfig, declare: Declare) -> Result<Self> {
        Self::with(
            &config.llm_base_url,
            &config.llm_model,
            config.llm_key.expose(),
            declare,
        )
    }

    fn with(base_url: &str, model: &str, key: &str, declare: Declare) -> Result<Self> {
        Ok(Self {
            client: http_client()?,
            url: format!("{}/chat/completions", openai_base(base_url)),
            auth: format!("Bearer {key}"),
            model: model.to_owned(),
            declare,
        })
    }

    /// The request's body, with the tools declared now.
    fn body(&self, messages: &[Message]) -> Value {
        let mut body = json!({"model": self.model, "stream": true, "messages": messages});
        let tools = functions(&(self.declare)());
        if !tools.is_empty() {
            body["tools"] = Value::Array(tools);
        }
        body
    }

    pub async fn send(&self, messages: &[Message]) -> Result<ChatStream> {
        let body = self.body(messages);
        let response = post(
            &self.client,
            &self.url,
            ("authorization", &self.auth),
            &body,
        )
        .await?;
        Ok(ChatStream {
            response,
            lines: Lines::default(),
            deltas: Deltas::default(),
            ready: Vec::new(),
            done: false,
        })
    }
}

pub struct ChatStream {
    response: reqwest::Response,
    lines: Lines,
    deltas: Deltas,
    /// Decoded events not yet handed out, newest last.
    ready: Vec<ChatEvent>,
    done: bool,
}

impl ChatStream {
    /// The next event; `None` after `Done`. Each wait for the service is
    /// bounded by the client's stall limit.
    pub async fn next(&mut self) -> Result<Option<ChatEvent>> {
        loop {
            if !self.ready.is_empty() {
                return Ok(Some(self.ready.remove(0)));
            }
            if self.done {
                return Ok(None);
            }
            match chunk(&mut self.response).await? {
                Some(bytes) => {
                    for data in self.lines.push(&bytes) {
                        self.ready.extend(self.deltas.data(&data)?);
                    }
                    if self.lines.partial.len() > MAX_LINE {
                        return Err(Error::Protocol("chat stream line length"));
                    }
                }
                // ponytail: an end without `[DONE]` counts as one; a cut
                // stream then looks finished. Treat it as an error if a
                // proxy is seen to drop streams.
                None => self.ready.extend(self.deltas.data("[DONE]")?),
            }
            self.done |= self.ready.last() == Some(&ChatEvent::Done);
        }
    }
}

/// No completion chunk comes near this; a stream without line ends does.
const MAX_LINE: usize = 1 << 20;

/// Server-sent events' `data:` payloads from bytes cut anywhere.
#[derive(Default)]
pub(crate) struct Lines {
    partial: Vec<u8>,
}

impl Lines {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.partial.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(end) = self.partial.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            if let Some(data) = line.trim_end_matches(['\r', '\n']).strip_prefix("data:") {
                out.push(data.trim_start().to_owned());
            }
        }
        out
    }
}

#[derive(Default)]
struct Pending {
    id: String,
    name: String,
    arguments: String,
}

/// Chunks of the completion to events. Tool calls arrive in fragments
/// keyed by `index` and are handed out whole at the end.
#[derive(Default)]
pub(crate) struct Deltas {
    calls: Vec<Pending>,
}

impl Deltas {
    pub fn data(&mut self, data: &str) -> Result<Vec<ChatEvent>> {
        if data == "[DONE]" {
            let mut out = Vec::new();
            if !self.calls.is_empty() {
                out.push(ChatEvent::ToolCalls(self.take_calls()?));
            }
            out.push(ChatEvent::Done);
            return Ok(out);
        }
        let chunk: Value =
            serde_json::from_str(data).map_err(|_| Error::Protocol("chat stream data"))?;
        let mut out = Vec::new();
        let Some(delta) = chunk.pointer("/choices/0/delta") else {
            return Ok(out);
        };
        if let Some(text) = delta["content"].as_str().filter(|t| !t.is_empty()) {
            out.push(ChatEvent::Text(text.to_owned()));
        }
        for call in delta["tool_calls"].as_array().into_iter().flatten() {
            let index = call["index"].as_u64().unwrap_or(0) as usize;
            if index >= 64 {
                return Err(Error::Protocol("tool call index"));
            }
            if self.calls.len() <= index {
                self.calls.resize_with(index + 1, Pending::default);
            }
            let pending = &mut self.calls[index];
            if let Some(id) = call["id"].as_str() {
                pending.id = id.to_owned();
            }
            if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
                pending.name.push_str(name);
            }
            if let Some(part) = call.pointer("/function/arguments").and_then(Value::as_str) {
                pending.arguments.push_str(part);
            }
        }
        Ok(out)
    }

    fn take_calls(&mut self) -> Result<Vec<ToolCall>> {
        std::mem::take(&mut self.calls)
            .into_iter()
            .filter(|call| !call.name.is_empty())
            .map(|call| {
                let args = if call.arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&call.arguments)
                        .map_err(|_| Error::Protocol("tool call arguments"))?
                };
                Ok(ToolCall {
                    id: call.id,
                    name: call.name,
                    args,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Feed recorded bytes cut at every `|`.
    fn events(recorded: &str) -> Vec<ChatEvent> {
        let mut lines = Lines::default();
        let mut deltas = Deltas::default();
        recorded
            .split('|')
            .flat_map(|piece| lines.push(piece.as_bytes()))
            .flat_map(|data| deltas.data(&data).unwrap())
            .collect()
    }

    #[test]
    fn text_split_across_chunks_comes_out_whole() {
        let recorded = "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\r\n\r\n\
            da|ta: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n\
            data: {\"choices\":[{\"del|ta\":{\"content\":\"lo.\"}}]}\n\n\
            : keep-alive comment\n\n\
            data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
            data: [DO|NE]\n\n";
        assert_eq!(
            events(recorded),
            [
                ChatEvent::Text("Hel".into()),
                ChatEvent::Text("lo.".into()),
                ChatEvent::Done
            ]
        );
    }

    #[test]
    fn tool_call_fragments_are_joined_by_index() {
        let recorded = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_a","type":"function","function":{"name":"the_time","arguments":""}}]}}]}
|
data: {"choices":[{"delta":{"tool_calls":[{"index":1,"id":"call_b","function":{"name":"remember","arguments":"{\"fa"}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"ct\": \"likes "}}]}}]}

data: {"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"arguments":"tea\"}"}}]}}]}

data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}

data: [DONE]
"#;
        assert_eq!(
            events(recorded),
            [
                ChatEvent::ToolCalls(vec![
                    ToolCall {
                        id: "call_a".into(),
                        name: "the_time".into(),
                        args: json!({}),
                    },
                    ToolCall {
                        id: "call_b".into(),
                        name: "remember".into(),
                        args: json!({"fact": "likes tea"}),
                    },
                ]),
                ChatEvent::Done
            ]
        );
    }

    /// A tool the executive offers after the worker started is declared
    /// from the next request on, and one it takes away is not.
    #[test]
    fn the_tools_are_declared_as_they_stand_at_each_request() {
        let offered: Arc<std::sync::Mutex<Vec<lamp_gemini::Tool>>> = Arc::default();
        let now = offered.clone();
        let chat = Chat::with(
            "http://127.0.0.1:9/v1/",
            "m",
            "k",
            Arc::new(move || now.lock().unwrap().clone()),
        )
        .unwrap();
        let asked = [Message::user("Lights on.")];
        assert!(chat.body(&asked).get("tools").is_none());
        *offered.lock().unwrap() = serde_json::from_value(json!([
            {"name": "light_on", "description": "Light on.", "parameters": {"type": "object", "properties": {}}}
        ]))
        .unwrap();
        assert_eq!(
            chat.body(&asked)["tools"][0]["function"]["name"],
            "light_on"
        );
        offered.lock().unwrap().clear();
        assert!(chat.body(&asked).get("tools").is_none());
    }

    #[test]
    fn broken_arguments_are_an_error() {
        let mut deltas = Deltas::default();
        deltas
            .data(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c","function":{"name":"x","arguments":"{"}}]}}]}"#)
            .unwrap();
        assert_eq!(
            deltas.data("[DONE]").unwrap_err(),
            Error::Protocol("tool call arguments")
        );
    }

    #[test]
    fn a_follow_up_carries_the_calls_and_their_results() {
        let call = ToolCall {
            id: "call_a".into(),
            name: "the_time".into(),
            args: json!({"zone": "local"}),
        };
        let messages = [
            Message::assistant_calls(Some(String::new()), std::slice::from_ref(&call)),
            Message::tool("call_a", "10:42"),
        ];
        assert_eq!(
            serde_json::to_value(messages).unwrap(),
            json!([
                {"role": "assistant", "tool_calls": [{"id": "call_a", "type": "function",
                    "function": {"name": "the_time", "arguments": "{\"zone\":\"local\"}"}}]},
                {"role": "tool", "content": "10:42", "tool_call_id": "call_a"},
            ])
        );
    }

    #[test]
    fn tools_are_declared_as_functions() {
        let tools: Vec<lamp_gemini::Tool> = serde_json::from_value(json!([
            {"name": "the_time", "description": "Now."},
            {"name": "remember", "description": "Keep a fact.",
             "parameters": {"type": "object", "properties": {"fact": {"type": "string"}}}},
        ]))
        .unwrap();
        assert_eq!(
            functions(&tools),
            [
                json!({"type": "function", "function": {"name": "the_time", "description": "Now.",
                    "parameters": {"type": "object", "properties": {}}}}),
                json!({"type": "function", "function": {"name": "remember", "description": "Keep a fact.",
                    "parameters": {"type": "object", "properties": {"fact": {"type": "string"}}}}}),
            ]
        );
    }

    /// A local plain HTTP server sends a recorded stream in two pieces.
    #[tokio::test]
    async fn a_streamed_reply_is_read_over_http() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !String::from_utf8_lossy(&request).contains("\"stream\":true") {
                let n = socket.read(&mut buffer).await.unwrap();
                request.extend_from_slice(&buffer[..n]);
            }
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n\
                      data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\nda",
                )
                .await
                .unwrap();
            socket.flush().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            socket
                .write_all(b"ta: {\"choices\":[{\"delta\":{\"content\":\" there.\"}}]}\n\ndata: [DONE]\n\n")
                .await
                .unwrap();
            String::from_utf8(request).unwrap()
        });
        let chat = Chat::with(
            &format!("http://{address}/v1/"),
            "m",
            "secret",
            Arc::new(Vec::new),
        )
        .unwrap();
        let mut stream = chat.send(&[Message::user("hello")]).await.unwrap();
        let mut got = Vec::new();
        while let Some(event) = stream.next().await.unwrap() {
            got.push(event);
        }
        assert_eq!(
            got,
            [
                ChatEvent::Text("Hi".into()),
                ChatEvent::Text(" there.".into()),
                ChatEvent::Done
            ]
        );
        let request = server.await.unwrap().to_lowercase();
        assert!(request.starts_with("post /v1/chat/completions http/1.1"));
        assert!(request.contains("authorization: bearer secret"));
        assert!(!request.contains("\"tools\""));
    }
}
