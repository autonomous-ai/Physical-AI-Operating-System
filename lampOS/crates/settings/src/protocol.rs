//! One request and one reply per private Unix stream. Mutations never use
//! lossy Bus events, and an interrupted exchange is not a rollback receipt.
use crate::model::Mutation;
use serde::{Deserialize, Serialize};

pub const MAX_REQUEST_BYTES: usize = 8192;
pub const MAX_REPLY_BYTES: usize = crate::store::MAX_STATE_BYTES + 4096;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// Authoritative snapshot, declaration, revision and reliability status.
    Read {},
    /// Small health/progress report without setting values.
    Status {},
    /// One atomic compare-and-set. Retry with the same complete mutation.
    Set { mutation: Mutation },
}

impl Request {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("too_long");
        }
        serde_json::from_slice(bytes).map_err(|_| "bad_request")
    }
}

pub fn refusal(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({"ok":false,"error":code,"message":message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_bounded_requests_have_no_hidden_apply_or_authority_flag() {
        assert_eq!(Request::parse(br#"{"op":"read"}"#), Ok(Request::Read {}));
        for bytes in [
            br#"{"op":"read","apply":true}"#.as_slice(),
            br#"{"op":"read","op":"status"}"#,
            br#"{"op":"set","mutation":null}"#,
            br#"{"op":"status"}{}"#,
            &[255],
        ] {
            assert_eq!(Request::parse(bytes), Err("bad_request"));
        }
        assert_eq!(
            Request::parse(&vec![b' '; MAX_REQUEST_BYTES + 1]),
            Err("too_long")
        );
    }
}
