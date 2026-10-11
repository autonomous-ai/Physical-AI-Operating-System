# First user session

This is a supervised desk-chat pilot: one seated person, Lamp fixed to the left
or right, speech and light enabled, motors disabled. Use the qualified revision
and configuration from [the operator runbook](attended-4ace-pilot.md). Main must
record the actual installed identity before starting; this sheet is not a start
command or evidence that an untested build is ready.

## Before inviting the user

The facilitator and Main first check the actual physical mute switch: start
muted, confirm no listening or speech; unmute and confirm fresh readiness; ask a
question; mute during an answer and confirm it stops; reopen and confirm a new
question works without stale audio. Record every outcome and the session's
metadata. A failed or unknown cleanup ends the session. Main also retains a
working service Stop path. These physical checks remain pending until performed.

Check that the whole base is supported and the voice is comfortable at the
person's chair. Keep Lamp stationary. Explain: "This build is for conversation.
It does not run work tasks yet. Tell us whenever it misunderstands you, takes too
long, cuts you off, or speaks when it shouldn't."

## Ten-minute conversation

Let the person use their own words and normal pace. Do not coach them to speak
louder, stretch their pauses or repeat a failed question until a test passes.

| Try | What to observe |
|---|---|
| A greeting and a short workday conversation | Does the reply fit? Is the wait noticeable? |
| A follow-up without repeating the subject | Does Lamp retain the local conversation context? |
| A sentence with a natural pause or self-correction | Is the whole thought answered? Keep cutoffs in the record. |
| A longer question and a request for a shorter answer | Is the response useful, appropriately brief and audible throughout? |
| A return to an earlier point | Does Lamp remember what was actually said, without inventing details? |

Let the user mention anything else that feels unnatural: repeated phrases,
unnecessary questions, the ring, volume, or tone. Do not score a light cue or a
spoken preamble as the start of a useful answer.

The current baseline suppresses new input while Lamp speaks; natural topic-change
interruption is not qualified. Addressee detection is also not implemented in
this pilot. Keep calls and conversations with other people muted. Testing those
limits is a separate observed scenario, not a capability demonstration. Camera
understanding and body/head movement are not enabled in this profile.

## Keep one record per session

- Session ID, exact runtime/configuration, time, desk placement and facilitator.
- Physical mute/reopen and Stop results, including any failure.
- The user's exact question and the response or missing response for each issue.
- Missed words, split thoughts, unwanted responses, audio gaps and repetitions.
- The user's perceived pace and comfort; label subjective feedback as such.
- One answer to: "Would you want to keep talking to Lamp while you work? Why?"

Keep failed attempts. Software timestamps and perceived delay are separate from
speech-end-to-first-audible-word measurements. Do not invent acoustic scores or
mark a recording as listened to unless someone actually reviewed it. Follow the
operator runbook to stop and verify release at the end; no automatic restart or
unattended handoff is part of this session.
