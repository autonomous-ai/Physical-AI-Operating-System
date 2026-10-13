//! Function calls: declared at setup, owned by the committed request,
//! answered by identifier, and never left for the service to wait on.
use super::*;
use crate::{Tool, ToolCall};

fn tool(name: &str) -> Tool {
    Tool {
        name: name.into(),
        description: "Turn the light on.".into(),
        parameters: None,
    }
}
fn call(id: &str, name: &str, args: Value) -> Value {
    json!({"toolCall":{"functionCalls":[{"id":id,"name":name,"args":args}]}})
}
async fn called(connection: &mut Connection, request: RequestId) -> ToolCall {
    match event(connection).await {
        Event::ToolCall { lineage, call } => {
            assert_eq!(lineage.request, request);
            assert_eq!(lineage.session.get(), 17);
            call
        }
        other => panic!("expected a function call, got {other:?}"),
    }
}

#[tokio::test]
async fn declared_tools_are_sent_at_setup_beside_search_and_not_at_all_when_there_are_none() {
    let plain = wire::encode_setup(&configuration(Timeouts::default())).unwrap();
    assert!(
        serde_json::from_str::<Value>(&plain).unwrap()["setup"]
            .get("tools")
            .is_none()
    );

    let colour = Tool {
        name: "light_colour".into(),
        description: "Set the light's colour.".into(),
        parameters: Some(
            json!({"type":"object","properties":{"colour":{"type":"string","enum":["red","blue"]}},"required":["colour"]}),
        ),
    };
    let configuration = configuration(Timeouts::default())
        .search(true)
        .tools(vec![tool("light_on"), colour.clone()])
        .unwrap();
    let setup: Value = serde_json::from_str(&wire::encode_setup(&configuration).unwrap()).unwrap();
    assert_eq!(
        setup["setup"]["tools"],
        json!([
            {"functionDeclarations":[
                {"name":"light_on","description":"Turn the light on."},
                {"name":"light_colour","description":"Set the light's colour.","parameters":colour.parameters}
            ]},
            {"googleSearch":{}}
        ])
    );
}

#[test]
fn a_tool_must_have_a_plain_name_a_description_and_object_parameters_and_names_do_not_repeat() {
    let base = || configuration(Timeouts::default());
    assert!(base().tools(vec![tool("a"), tool("b_2")]).is_ok());
    for name in ["", "Light", "2go", "light-on", "light on", &"a".repeat(49)] {
        assert!(base().tools(vec![tool(name)]).is_err(), "{name:?}");
    }
    assert!(base().tools(vec![tool("same"), tool("same")]).is_err());
    assert!(
        base()
            .tools(
                (0..=crate::MAX_TOOLS)
                    .map(|n| tool(&format!("t{n}")))
                    .collect()
            )
            .is_err()
    );
    let with = |parameters: Value, description: &str| Tool {
        name: "x".into(),
        description: description.into(),
        parameters: Some(parameters),
    };
    assert!(
        base()
            .tools(vec![with(json!({"type":"object"}), "d")])
            .is_ok()
    );
    assert!(base().tools(vec![with(json!("string"), "d")]).is_err());
    assert!(
        base()
            .tools(vec![with(json!({"type":"object"}), " ")])
            .is_err()
    );
    assert!(
        base()
            .tools(vec![with(json!({"type":"object"}), &"d".repeat(1_025))])
            .is_err()
    );
    assert!(
        base()
            .tools(vec![with(
                json!({"pad":"p".repeat(crate::MAX_TOOL_JSON_BYTES)}),
                "d"
            )])
            .is_err()
    );
}

/// The ordinary course: a question is committed, the service calls a
/// function for it, the call is answered by identifier, and the answer the
/// service then speaks is the same request's.
#[tokio::test]
async fn a_call_belongs_to_the_committed_request_and_its_answer_goes_back_by_identifier() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    send_json(
        &mut server,
        call("call-1", "light_colour", json!({"colour":"blue"})),
    )
    .await;
    let made = called(&mut connection, request).await;
    assert_eq!(
        made,
        ToolCall {
            id: "call-1".into(),
            name: "light_colour".into(),
            args: json!({"colour":"blue"}),
        }
    );
    // Nothing is written until the call is answered.
    silent(&mut server).await;
    connection
        .input()
        .try_tool_response("call-1", "light_colour", &json!({"light":"blue"}))
        .unwrap();
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"call-1","name":"light_colour","response":{"light":"blue"}}
        ]}})
    );
    send_json(&mut server, audio(5)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, pcm, .. } if lineage.request == request && pcm == [5]
    ));
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request
    ));
    // Answered once: a second answer has no call to go to.
    connection
        .input()
        .try_tool_response("call-1", "light_colour", &json!({"light":"blue"}))
        .unwrap();
    discarded(&mut connection, Discard::StaleToolResponse).await;
    silent(&mut server).await;
}

/// Several calls in one message, a call without arguments or identifier,
/// and an answer that is not an object.
#[tokio::test]
async fn calls_arrive_in_order_and_shapes_the_service_may_leave_out_are_filled_in() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    send_json(
        &mut server,
        json!({"toolCall":{"functionCalls":[
            {"id":"a","name":"volume_up"},
            {"name":"what_time","args":null}
        ]}}),
    )
    .await;
    let first = called(&mut connection, request).await;
    let second = called(&mut connection, request).await;
    assert_eq!((first.id.as_str(), first.name.as_str()), ("a", "volume_up"));
    assert_eq!(first.args, json!({}));
    assert_eq!(
        (second.id.as_str(), second.name.as_str()),
        ("", "what_time")
    );
    assert_eq!(second.args, json!({}));
    let input = connection.input();
    input
        .try_tool_response("", "what_time", &json!("19:54"))
        .unwrap();
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"name":"what_time","response":{"result":"19:54"}}
        ]}})
    );
    input
        .try_tool_response("a", "volume_up", &json!({"volume":60}))
        .unwrap();
    assert_eq!(
        receive(&mut server).await["toolResponse"]["functionResponses"][0]["id"],
        "a"
    );
    // An answer too large to send is refused where it is offered.
    assert_eq!(
        input.try_tool_response(
            "a",
            "volume_up",
            &json!({"pad":"p".repeat(crate::MAX_TOOL_JSON_BYTES)})
        ),
        Err(Error::MessageTooLarge)
    );
    assert_eq!(connection.state(), State::Ready);
}

/// The service waits on every call it makes. One that arrives with no
/// committed request to own it is answered at once as cancelled, on the
/// wire, and is not reported as a call: nobody could act on it.
#[tokio::test]
async fn a_call_nobody_can_own_is_answered_as_cancelled_and_not_reported() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    send_json(&mut server, call("stray", "light_on", json!({}))).await;
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"stray","name":"light_on","response":{"error":"cancelled"}}
        ]}})
    );
    discarded(&mut connection, Discard::UnownedToolCall).await;
    quiet(&mut connection).await;

    // The same while a request is still being spoken: its input is open,
    // so nothing the service sends can be for it.
    let input = connection.input();
    input.try_start(request(1)).unwrap();
    sent(&mut server, "activityStart").await;
    assert!(matches!(
        event(&mut connection).await,
        Event::InputStarted { .. }
    ));
    send_json(&mut server, call("early", "light_on", json!({}))).await;
    assert_eq!(
        receive(&mut server).await["toolResponse"]["functionResponses"][0]["response"],
        json!({"error":"cancelled"})
    );
    discarded(&mut connection, Discard::UnownedToolCall).await;
    assert_eq!(connection.state(), State::Ready);
}

/// The person speaks over an answer that was waiting on a call. The service
/// withdraws the call; the answer that then comes for it is dropped and
/// nothing of it reaches the wire.
#[tokio::test]
async fn a_withdrawn_call_is_reported_and_its_late_answer_is_dropped() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    send_json(
        &mut server,
        call("call-1", "play_scene", json!({"scene":"focus"})),
    )
    .await;
    called(&mut connection, request).await;
    send_json(
        &mut server,
        json!({"toolCallCancellation":{"ids":["call-1"]}}),
    )
    .await;
    assert!(matches!(
        event(&mut connection).await,
        Event::ToolCallsCancelled { session, ids } if session.get() == 17 && ids == ["call-1"]
    ));
    connection
        .input()
        .try_tool_response("call-1", "play_scene", &json!({"ok":true}))
        .unwrap();
    discarded(&mut connection, Discard::StaleToolResponse).await;
    silent(&mut server).await;
    assert_eq!(connection.state(), State::Ready);
}

/// A call is the service doing something for the request: it satisfies the
/// wait for a first output, and the silence the service is allowed after it
/// runs from the call and again from its answer. A call that is never
/// answered ends the connection at the stall bound, by name.
#[tokio::test(start_paused = true)]
async fn a_call_counts_as_output_and_one_never_answered_is_a_stalled_response() {
    let timeouts = Timeouts {
        response: Duration::from_secs(3),
        stall: Duration::from_secs(1),
        ..Timeouts::default()
    };
    let (mut connection, mut server) = mock(timeouts).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    tokio::time::sleep(Duration::from_millis(2_500)).await;
    send_json(&mut server, call("call-1", "light_on", json!({}))).await;
    called(&mut connection, request).await;
    // Past the response bound, measured from the end of input: no failure.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(connection.state(), State::Ready);
    connection
        .input()
        .try_tool_response("call-1", "light_on", &json!({"light":"on"}))
        .unwrap();
    assert!(receive(&mut server).await.get("toolResponse").is_some());
    // The stall bound now runs from the answer, not from the call.
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert_eq!(connection.state(), State::Ready);
    send_json(&mut server, call("call-2", "light_off", json!({}))).await;
    called(&mut connection, request).await;
    assert_eq!(disconnected(&connection).await, Error::StalledResponse);
}

#[test]
fn a_call_message_outside_its_bounds_is_refused_by_the_codec() {
    let decode = |value: Value| wire::decode_server(value.to_string().as_bytes());
    assert!(decode(call("ok", "light_on", json!({}))).is_ok());
    // No calls in a call message.
    assert_eq!(
        decode(json!({"toolCall":{"functionCalls":[]}})).unwrap_err(),
        Error::MalformedMessage
    );
    // A call beside content in one message.
    assert_eq!(
        decode(json!({"toolCall":{"functionCalls":[{"name":"x"}]},"serverContent":{}}))
            .unwrap_err(),
        Error::MalformedMessage
    );
    let many: Vec<Value> = (0..=crate::MAX_TOOL_CALLS)
        .map(|n| json!({"id":n.to_string(),"name":"x"}))
        .collect();
    for oversized in [
        json!({"toolCall":{"functionCalls":many}}),
        call(&"i".repeat(crate::MAX_TOOL_ID_BYTES + 1), "x", json!({})),
        call("id", &"n".repeat(65), json!({})),
        call(
            "id",
            "x",
            json!({"pad":"p".repeat(crate::MAX_TOOL_JSON_BYTES)}),
        ),
        json!({"toolCallCancellation":{"ids":["i".repeat(crate::MAX_TOOL_ID_BYTES + 1)]}}),
    ] {
        assert_eq!(decode(oversized).unwrap_err(), Error::MessageTooLarge);
    }
    let frame = decode(json!({"toolCallCancellation":{"ids":["a","b"]}})).unwrap();
    assert_eq!(frame.tool_cancellations, ["a", "b"]);
    assert!(frame.tool_calls.is_empty());
}
