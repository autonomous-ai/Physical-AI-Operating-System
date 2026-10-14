//! Opt-in requests, plus the unchanged legacy response shapes. Model payloads
//! are data for an offline caller; no request here invokes inference.
use crate::{
    days::{Days, Error, Result},
    protocol::{self, Request, Response, Who},
    store::{Ask, Day, Person},
    tidy,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Extra {
    AppendLinked {
        #[serde(default)]
        person: Who,
        at_unix_ms: u64,
        #[serde(default)]
        heard: String,
        #[serde(default)]
        said: String,
        source_ids: Vec<u64>,
    },
    Search {
        #[serde(default)]
        person: Who,
        query: String,
        #[serde(default)]
        day: Option<String>,
        #[serde(default)]
        limit: Option<u64>,
        #[serde(default)]
        max_bytes: Option<u64>,
        #[serde(default)]
        history: bool,
    },
    Forget {
        #[serde(default)]
        person: Who,
        id: u64,
    },
    SessionOpen {
        #[serde(default)]
        person: Who,
        session: String,
        #[serde(default)]
        max_bytes: Option<u64>,
    },
    SessionClose {
        #[serde(default)]
        person: Who,
        session: String,
    },
    ConversationEnd {
        #[serde(default)]
        person: Who,
        conversation: String,
        source_ids: Vec<u64>,
        #[serde(default)]
        previous_ids: Vec<u64>,
    },
    TidyApply {
        #[serde(default)]
        person: Who,
        job: String,
        observations: Vec<tidy::Proposal>,
    },
}

fn one(person: Who) -> Result<Person> {
    match person {
        Who::Unsaid => Ok(Person::everyone()),
        Who::Id(id) => Person::new(&id)
            .ok_or_else(|| Error::new("bad_person", "a person ID is 1 to 32 of a-z, 0-9, _ and -")),
        Who::All => Err(Error::new(
            "bad_person",
            "only status and forget_all accept a null person",
        )),
    }
}
fn some(person: Who, unsaid_all: bool) -> Result<Option<Person>> {
    match person {
        Who::All => Ok(None),
        Who::Unsaid if unsaid_all => Ok(None),
        p => one(p).map(Some),
    }
}
fn ask<'a>(
    query: Option<&'a str>,
    day: Option<&str>,
    limit: Option<u64>,
    bytes: Option<u64>,
) -> Result<Ask<'a>> {
    let day = day
        .map(|d| {
            Day::parse(d)
                .ok_or_else(|| Error::new("bad_day", "a day is YYYY-MM-DD from 1970 through 9999"))
        })
        .transpose()?;
    Ok(Ask {
        query,
        day,
        limit: limit.unwrap_or(8).min(50) as usize,
        max_bytes: bytes.unwrap_or(4096).min(16384) as usize,
    })
}
fn value(response: Response) -> Value {
    serde_json::to_value(response).expect("the response contains JSON values only")
}

fn legacy(store: &mut Days, request: Request) -> Result<Value> {
    Ok(match request {
        Request::Append {
            person,
            at_unix_ms,
            heard,
            said,
        } => value(Response::Added {
            id: store.append(&one(person)?, at_unix_ms, &heard, &said, &[])?,
        }),
        Request::Remember {
            person,
            at_unix_ms,
            text,
        } => value(Response::Added {
            id: store.remember(&one(person)?, at_unix_ms, &text)?,
        }),
        Request::Kept { person, max_bytes } => value(Response::Kept {
            entries: store.kept(&one(person)?, max_bytes.unwrap_or(4096).min(16384) as usize)?,
        }),
        Request::Recall {
            person,
            query,
            day,
            limit,
            max_bytes,
        } => value(Response::Recalled {
            entries: store.recall(
                &one(person)?,
                &ask(query.as_deref(), day.as_deref(), limit, max_bytes)?,
            )?,
        }),
        Request::ForgetLast { person, kind } => value(Response::Forgot {
            removed: store.forget_last(&one(person)?, kind)?,
        }),
        Request::ForgetAll { person } => value(Response::Emptied {
            removed: store.forget_all(some(person, false)?.as_ref())?,
        }),
        Request::Status { person } => value(Response::Status {
            people: store.status(some(person, true)?.as_ref())?,
        }),
    })
}
fn extra(store: &mut Days, request: Extra) -> Result<Value> {
    Ok(match request {
        Extra::AppendLinked {
            person,
            at_unix_ms,
            heard,
            said,
            source_ids,
        } => {
            json!({"ok":true,"id":store.append(&one(person)?,at_unix_ms,&heard,&said,&source_ids)?})
        }
        Extra::Search {
            person,
            query,
            day,
            limit,
            max_bytes,
            history,
        } => {
            json!({"ok":true,"entries":store.search(&one(person)?,&ask(Some(&query),day.as_deref(),limit,max_bytes)?,history)?})
        }
        Extra::Forget { person, id } => {
            json!({"ok":true,"removed":store.forget(&one(person)?,id)?})
        }
        Extra::SessionOpen {
            person,
            session,
            max_bytes,
        } => {
            json!({"ok":true,"snapshot":store.session_open(&one(person)?,&session,max_bytes.unwrap_or(1200).min(1200) as usize)?})
        }
        Extra::SessionClose { person, session } => {
            store.session_close(&one(person)?, &session)?;
            json!({"ok":true})
        }
        Extra::ConversationEnd {
            person,
            conversation,
            source_ids,
            previous_ids,
        } => {
            json!({"ok":true,"input":store.conversation_end_with(&one(person)?,&conversation,&source_ids,&previous_ids)?})
        }
        Extra::TidyApply {
            person,
            job,
            observations,
        } => {
            json!({"ok":true,"ids":store.tidy_apply(&one(person)?,&job,&tidy::Output{observations})?})
        }
    })
}

pub fn answer(store: &mut Days, line: &[u8], now: u64) -> protocol::Answer {
    let mut op = "?".to_owned();
    let mut who = "?".to_owned();
    let result = (|| -> Result<Value> {
        if line.len() > protocol::MOST_LINE_BYTES {
            return Err(Error::new("too_long", "a request line is at most 64 KiB"));
        }
        store.maintain(now)?;
        let parsed: Value = serde_json::from_slice(line)
            .map_err(|_| Error::new("bad_json", "the request is not JSON"))?;
        let name = parsed
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::new("bad_request", "a request needs an op"))?;
        const NEW: &[&str] = &[
            "append_linked",
            "search",
            "forget",
            "session_open",
            "session_close",
            "conversation_end",
            "tidy_apply",
        ];
        const OLD: &[&str] = &[
            "append",
            "remember",
            "kept",
            "recall",
            "forget_last",
            "forget_all",
            "status",
        ];
        if !NEW.contains(&name) && !OLD.contains(&name) {
            return Err(Error::new("unknown_op", "the memory operation is unknown"));
        }
        op = name.into();
        who = match parsed.get("person") {
            None => Person::EVERYONE.into(),
            Some(Value::Null) => "*".into(),
            Some(Value::String(s)) if Person::new(s).is_some() => s.clone(),
            _ => "?".into(),
        };
        if NEW.contains(&name) {
            extra(
                store,
                serde_json::from_value(parsed)
                    .map_err(|_| Error::new("bad_request", "invalid fields for this operation"))?,
            )
        } else {
            legacy(
                store,
                serde_json::from_value(parsed)
                    .map_err(|_| Error::new("bad_request", "invalid fields for this operation"))?,
            )
        }
    })();
    let (response, outcome) = match result {
        Ok(response) => {
            let count = response
                .get("entries")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            let outcome = format!("ok entries={count}");
            (response, outcome)
        }
        Err(e) => (
            json!({"ok":false,"error":e.code,"message":e.message}),
            format!("refused error={}", e.code),
        ),
    };
    protocol::Answer {
        line: response.to_string(),
        log: format!("{op} person={who} {outcome}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_opt_in_request_shape_in_the_document_parses() {
        let document = include_str!("../../../docs/memory.md");
        let (_, optional) = document
            .split_once("### Additional requests with `--memory-days`")
            .unwrap();
        let mut names = Vec::new();
        for line in optional
            .lines()
            .map(str::trim)
            .filter(|s| s.starts_with("{\"op\""))
        {
            let request: Value = serde_json::from_str(line).unwrap();
            names.push(request["op"].as_str().unwrap().to_owned());
            serde_json::from_value::<Extra>(request).unwrap();
        }
        assert_eq!(
            names,
            [
                "search",
                "append_linked",
                "forget",
                "session_open",
                "session_close",
                "conversation_end",
                "tidy_apply"
            ]
        );
    }
}
