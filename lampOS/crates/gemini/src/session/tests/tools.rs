//! Function calls: declared at setup, owned by the committed request,
//! answered by identifier, and never left for the service to wait on.
use super::*;
use crate::{Tool, ToolCall};

fn tool(name: &str) -> Tool {
    Tool {
        name: name.into(),
        description: "Turn the light on.".into(),
        parameters: None,
        silent: false,
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
        silent: false,
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
        silent: false,
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
            carried_from: None,
        }
    );
    // The service ends its turn as soon as it has asked, as the real one
    // does. That is not the end of the request: nothing is reported.
    send_json(&mut server, generated()).await;
    send_json(&mut server, idle()).await;
    quiet(&mut connection).await;
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

/// The answer can be on its way before the service has completed the turn
/// it asked in: a function answered in a millisecond, a completion that
/// took four. The completion still belongs to the asking, and what the
/// service says next is still this request's.
#[tokio::test]
async fn a_completion_that_follows_the_answer_still_ends_only_the_asking() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    send_json(&mut server, call("call-1", "not_for_me", json!({}))).await;
    called(&mut connection, request).await;
    connection
        .input()
        .try_tool_response("call-1", "not_for_me", &json!({"ok":true}))
        .unwrap();
    assert!(receive(&mut server).await.get("toolResponse").is_some());
    send_json(&mut server, idle()).await;
    quiet(&mut connection).await;
    send_json(&mut server, audio(5)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, .. } if lineage.request == request
    ));
    send_json(&mut server, generated()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::GenerationComplete { lineage } if lineage.request == request
    ));
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request
    ));
    // The request is over: the next one is a request of its own.
    let next = ended_turn(&mut connection, &mut server, 2).await;
    send_json(&mut server, audio(6)).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::Audio { lineage, .. } if lineage.request == next
    ));
}

/// A function nothing is to be said on account of. It is declared as one
/// the service need not wait for and answered as one to take note of
/// without generating. The service says nothing more for the request, so
/// the turn it asked in is the request's last; and the answer is still
/// sent when it comes after that.
#[tokio::test]
async fn a_silent_function_is_declared_and_answered_as_one_and_its_request_ends_with_the_asking() {
    let quiet_one = Tool {
        name: "not_for_me".into(),
        description: "Not said to you.".into(),
        parameters: None,
        silent: true,
    };
    let configuration = configuration(Timeouts::default())
        .tools(vec![quiet_one, tool("light_on")])
        .unwrap();
    let setup: Value = serde_json::from_str(&wire::encode_setup(&configuration).unwrap()).unwrap();
    let declared = &setup["setup"]["tools"][0]["functionDeclarations"];
    assert_eq!(declared[0]["behavior"], "NON_BLOCKING");
    assert!(declared[1].get("behavior").is_none());

    let (mut connection, mut server) = mock_with(configuration).await;
    let request = ended_turn(&mut connection, &mut server, 1).await;
    send_json(&mut server, call("call-1", "not_for_me", json!({}))).await;
    called(&mut connection, request).await;
    // The completion of the asking turn is the request's own.
    send_json(&mut server, idle()).await;
    assert!(matches!(
        event(&mut connection).await,
        Event::TurnComplete { lineage, idle: true } if lineage.request == request
    ));
    // The answer comes after the request has ended, and is sent.
    connection
        .input()
        .try_tool_response("call-1", "not_for_me", &json!({"ok":true}))
        .unwrap();
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"call-1","name":"not_for_me","response":{"ok":true},"scheduling":"SILENT"}
        ]}})
    );
    // Once: a second answer has no call to go to.
    connection
        .input()
        .try_tool_response("call-1", "not_for_me", &json!({"ok":true}))
        .unwrap();
    discarded(&mut connection, Discard::StaleToolResponse).await;
    // A function that is not silent, on the same session, is still waited
    // for across its asking turn and answered plainly.
    let next = ended_turn(&mut connection, &mut server, 2).await;
    send_json(&mut server, call("call-2", "light_on", json!({}))).await;
    called(&mut connection, next).await;
    send_json(&mut server, idle()).await;
    quiet(&mut connection).await;
    connection
        .input()
        .try_tool_response("call-2", "light_on", &json!({"light":"on"}))
        .unwrap();
    assert!(
        receive(&mut server).await["toolResponse"]["functionResponses"][0]
            .get("scheduling")
            .is_none()
    );
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
/// committed request to own it is answered at once as not done, saying why,
/// on the wire, and is not reported as a call: nobody could act on it.
#[tokio::test]
async fn a_call_nobody_can_own_is_answered_as_cancelled_and_not_reported() {
    let (mut connection, mut server) = mock(Timeouts::default()).await;
    send_json(&mut server, call("stray", "light_on", json!({}))).await;
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"stray","name":"light_on","response":{"error":crate::NOT_DONE_STILL_SPEAKING}}
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
        json!({"error":crate::NOT_DONE_STILL_SPEAKING})
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
    send_json(&mut server, idle()).await;
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

/// "Yes." and, a moment later, "please start it." (lamp-4ace, 2026-10-14,
/// 22:37, #736): the first request is committed, then given up as continued
/// by the second, and the call the model makes for the first comes while the
/// second is still being heard. `carry` says whether the session carries it.
async fn two_breaths(carry: bool, abandon: bool) -> (Connection, MockSocket, RequestId) {
    two_breaths_with(carry, abandon, Timeouts::default()).await
}

async fn two_breaths_with(
    carry: bool,
    abandon: bool,
    timeouts: Timeouts,
) -> (Connection, MockSocket, RequestId) {
    let (mut connection, mut server) = mock_with(configuration(timeouts).carry_calls(carry)).await;
    let yes = ended_turn(&mut connection, &mut server, 1).await;
    let input = connection.input();
    if abandon {
        input.discard(yes);
    } else {
        input.retire(yes);
    }
    let going_on = request(2);
    input.try_start(going_on).unwrap();
    sent(&mut server, "activityStart").await;
    assert!(
        matches!(event(&mut connection).await, Event::InputStarted { lineage, .. } if lineage.request == going_on)
    );
    (connection, server, yes)
}

#[tokio::test]
async fn a_call_for_a_request_just_continued_is_carried_into_the_one_that_replaced_it() {
    let (mut connection, mut server, yes) = two_breaths(true, false).await;
    send_json(
        &mut server,
        call("confirm-1", "confirm_agent_task", json!({})),
    )
    .await;
    // Reported as the open request's call, marked as made for the first.
    let made = called(&mut connection, request(2)).await;
    assert_eq!(made.carried_from, Some(yes));
    assert_eq!(made.name, "confirm_agent_task");
    // Not answered as cancelled: the service hears nothing until it is
    // answered, and then the answer.
    silent(&mut server).await;
    connection
        .input()
        .try_tool_response("confirm-1", "confirm_agent_task", &json!({"started":true}))
        .unwrap();
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"confirm-1","name":"confirm_agent_task","response":{"started":true}}
        ]}})
    );
    // Answered once.
    connection
        .input()
        .try_tool_response("confirm-1", "confirm_agent_task", &json!({"started":true}))
        .unwrap();
    discarded(&mut connection, Discard::StaleToolResponse).await;
}

/// Without the setting the same call is answered as not done and dropped,
/// exactly as before.
#[tokio::test]
async fn without_carrying_the_same_call_is_answered_as_before() {
    let (mut connection, mut server, _) = two_breaths(false, false).await;
    send_json(
        &mut server,
        call("confirm-1", "confirm_agent_task", json!({})),
    )
    .await;
    assert_eq!(
        receive(&mut server).await,
        json!({"toolResponse":{"functionResponses":[
            {"id":"confirm-1","name":"confirm_agent_task","response":{"error":crate::NOT_DONE_STILL_SPEAKING}}
        ]}})
    );
    discarded(&mut connection, Discard::UnownedToolCall).await;
}

/// A request given up for something else (abandoned, not continued) keeps
/// nothing: its call is answered as not done even with the setting.
#[tokio::test]
async fn a_call_for_a_request_abandoned_is_not_carried() {
    let (mut connection, mut server, _) = two_breaths(true, true).await;
    send_json(&mut server, call("light-1", "light_on", json!({}))).await;
    assert_eq!(
        receive(&mut server).await["toolResponse"]["functionResponses"][0]["response"],
        json!({"error":crate::NOT_DONE_STILL_SPEAKING})
    );
    discarded(&mut connection, Discard::UnownedToolCall).await;
}

/// A call that comes later than the window after the request was replaced
/// belongs to something else, and is answered as not done. The barrier's
/// own quiet bound is set past the window here: at its default (2 s) the
/// session would end on the standing barrier before the call is read.
#[tokio::test]
async fn a_call_later_than_the_window_is_not_carried() {
    let timeouts = Timeouts {
        barrier: Duration::from_secs(5),
        ..Timeouts::default()
    };
    let (mut connection, mut server, _) = two_breaths_with(true, false, timeouts).await;
    tokio::time::pause();
    tokio::time::advance(crate::CARRIED_CALL_WINDOW + Duration::from_millis(100)).await;
    tokio::time::resume();
    send_json(&mut server, call("late-1", "light_on", json!({}))).await;
    assert_eq!(
        receive(&mut server).await["toolResponse"]["functionResponses"][0]["response"],
        json!({"error":crate::NOT_DONE_STILL_SPEAKING})
    );
    discarded(&mut connection, Discard::UnownedToolCall).await;
}
