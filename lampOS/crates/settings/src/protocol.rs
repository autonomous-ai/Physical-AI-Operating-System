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

/// Whether a reply confirms what its request asked. A bare `ok:true` is not a
/// commit: a set must name the guarded store and the revision after it, and
/// say that only the desired value was saved.
pub fn confirms(request: &Request, reply: &serde_json::Value) -> bool {
    if reply["ok"] != true || reply["reliable"] == false {
        return false;
    }
    match request {
        Request::Read {} => {
            reply["state"]["store_id"].is_string()
                && reply["state"]["revision"].is_u64()
                && reply["reliable"].is_boolean()
        }
        Request::Status {} => {
            reply["ready"].is_boolean()
                && reply["reliable"].is_boolean()
                && reply["store_id"].is_string()
                && reply["revision"].is_u64()
        }
        Request::Set { mutation } => {
            reply["application"] == "desired_only"
                && reply["commit"]["store_id"] == mutation.guard.store_id.as_str()
                && reply["commit"]["revision"].as_u64() == mutation.guard.revision.checked_add(1)
                && reply["commit"]["replayed"].is_boolean()
        }
    }
}

pub fn refusal(code: &str, message: &str) -> serde_json::Value {
    serde_json::json!({"ok":false,"error":code,"message":message})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entry, Guard, Value};
    use serde_json::json;

    fn set() -> Request {
        Request::Set {
            mutation: Mutation {
                guard: Guard {
                    store_id: "store-a".into(),
                    revision: 4,
                    request_id: "request-1".into(),
                },
                changes: vec![Entry {
                    name: "light.level".into(),
                    value: Value::Integer(3),
                }],
            },
        }
    }

    #[test]
    fn a_bare_ok_or_another_commit_does_not_confirm_a_set() {
        let good = json!({"ok":true,"application":"desired_only",
            "commit":{"store_id":"store-a","revision":5,"replayed":false}});
        assert!(confirms(&set(), &good));
        let mut replayed = good.clone();
        replayed["commit"]["replayed"] = json!(true);
        assert!(confirms(&set(), &replayed));
        for bad in [
            json!({"ok":true}),
            json!({"ok":true,"application":"desired_only"}),
            json!({"ok":false,"application":"desired_only",
                "commit":{"store_id":"store-a","revision":5,"replayed":false}}),
            json!({"ok":true,"application":"applied",
                "commit":{"store_id":"store-a","revision":5,"replayed":false}}),
            json!({"ok":true,"application":"desired_only",
                "commit":{"store_id":"store-b","revision":5,"replayed":false}}),
            json!({"ok":true,"application":"desired_only",
                "commit":{"store_id":"store-a","revision":4,"replayed":false}}),
            json!({"ok":true,"application":"desired_only",
                "commit":{"store_id":"store-a","revision":5}}),
            json!({"ok":true,"application":"desired_only","reliable":false,
                "commit":{"store_id":"store-a","revision":5,"replayed":false}}),
        ] {
            assert!(!confirms(&set(), &bad), "{bad}");
        }
    }

    #[test]
    fn reads_and_status_need_their_own_fields_and_an_honest_reliability() {
        let read = json!({"ok":true,"reliable":true,"state":{"store_id":"s","revision":0}});
        assert!(confirms(&Request::Read {}, &read));
        assert!(!confirms(
            &Request::Read {},
            &json!({"ok":true,"reliable":true})
        ));
        let mut unreliable = read.clone();
        unreliable["reliable"] = json!(false);
        assert!(!confirms(&Request::Read {}, &unreliable));
        let status = json!({"ok":true,"ready":true,"reliable":true,"store_id":"s","revision":2});
        assert!(confirms(&Request::Status {}, &status));
        assert!(!confirms(&Request::Status {}, &read));
    }

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
