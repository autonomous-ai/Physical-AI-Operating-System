//! The cached serve-conversation example and the plan rules it relies on.
//! Existing plan bytes, and so their hashes, stay unchanged.
use lamp_voice_eval::{
    plan::{DEFAULT_PLAN, LoadedPlan, MAX_STEPS, SERVE_PLAN, TriggerEvent},
    stimulus::StimulusCatalog,
};
use serde_json::Value;

#[test]
fn the_default_plan_hash_is_unchanged() {
    let catalog = StimulusCatalog::load().unwrap();
    let plan = LoadedPlan::parse(DEFAULT_PLAN, &catalog).unwrap();
    assert_eq!(
        plan.sha256,
        "e9dad734a420fc14c955e023bad49ea5f509809dd2c3079007ae89084a3023cc"
    );
}

#[test]
fn the_serve_example_is_eight_cached_context_linked_turns() {
    let catalog = StimulusCatalog::load().unwrap();
    let plan = LoadedPlan::parse(SERVE_PLAN, &catalog).unwrap();
    let scenario = &plan.plan.scenarios[0];
    assert_eq!(scenario.steps.len(), 8);
    // Every stimulus is a catalog scene with a cached asset identity; the
    // runner never renders speech.
    assert!(scenario.steps.iter().all(|s| {
        s.scene
            .as_deref()
            .is_some_and(|scene| catalog.scene(scene).is_ok())
    }));
    let triggers: Vec<_> = scenario.steps.iter().map(|s| s.trigger.after).collect();
    assert_eq!(
        triggers,
        [
            TriggerEvent::ListeningReady,
            TriggerEvent::NextTurnReady,
            TriggerEvent::NextTurnReady,
            TriggerEvent::SpeakerFirstWrite,
            TriggerEvent::NextTurnReady,
            TriggerEvent::LocalEndpoint,
            TriggerEvent::NextTurnReady,
            TriggerEvent::NextTurnReady,
        ]
    );
}

fn variant(edit: impl FnOnce(&mut Value)) -> String {
    let mut plan: Value = serde_json::from_str(SERVE_PLAN).unwrap();
    edit(&mut plan["scenarios"][0]["steps"]);
    plan.to_string()
}

#[test]
fn a_correction_must_follow_a_request_and_conversations_stop_at_twenty_turns() {
    let catalog = StimulusCatalog::load().unwrap();
    // The example's correction follows a request (a fragment); after a
    // step that expects no admitted request it is refused.
    let after_silence = variant(|steps| steps[4]["expect"] = "silence".into());
    let error = LoadedPlan::parse(&after_silence, &catalog).unwrap_err();
    assert!(error.to_string().contains("local_endpoint"), "{error}");

    let repeat = |count: usize| {
        variant(|steps| {
            let follow = steps[1].clone();
            let list = steps.as_array_mut().unwrap();
            list.truncate(2);
            for n in list.len()..count {
                let mut step = follow.clone();
                step["id"] = format!("follow-{n}").into();
                list.push(step);
            }
        })
    };
    let mut twenty: Value = serde_json::from_str(&repeat(MAX_STEPS)).unwrap();
    twenty["scenarios"][0]["session_seconds"] = 600.into();
    assert!(LoadedPlan::parse(&twenty.to_string(), &catalog).is_ok());
    let mut over: Value = serde_json::from_str(&repeat(MAX_STEPS + 1)).unwrap();
    over["scenarios"][0]["session_seconds"] = 600.into();
    let error = LoadedPlan::parse(&over.to_string(), &catalog).unwrap_err();
    assert!(error.to_string().contains("1..20 steps"), "{error}");
}
