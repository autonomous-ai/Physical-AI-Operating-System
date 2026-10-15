# First's lamp check for #749

No command in this procedure was run on a lamp by Fourth. First owns the
installation, permission for motion, compatibility minute and overnight watch.
Use the exact native ARM64 artifact named in the PR, verify its SHA-256, and
retain the installed unit/binary hash with the observations. A locally rebuilt
binary is a different build and must be identified as such.

1. Preserve the current binary, catalogue and unit. Install the CI artifact by
   the existing keeper/unit procedure, keeping the current catalogue unchanged
   (`alive.sleep.name_requires_speech` absent, or false only for the new binary).
   Run the required compatibility minute and retain the existing plan/body
   observations. Do not infer installed state from the checked-in deploy file.
2. Read process identity/restarts and retain a bounded record from that minute:

   ```sh
   systemctl show lamp-expression -p ActiveState -p SubState -p NRestarts -p ExecStart
   lamp-bus record /tmp/sleep-name-off voice/progress voice/activity name/heard expression/state body/joint_states --seconds 60 --max-mb 8
   journalctl -u lamp-expression --since '2 minutes ago' --no-pager -o short-iso-precise
   ```

   First should choose a new record directory if that path already exists.
   This recording needs no transcript or microphone audio. Expected: the same
   enabled state and activity as the old catalogue, no added ignored-name lines,
   no restart, and no new actuator behavior. Record the actual UTC interval.
3. After the minute, enable `alive.sleep.name_requires_speech: true` in the
   actual catalogue used by the unit and reload by the existing installation
   procedure. Keep every other setting, voice privacy and body calibration.
   At night let the lamp reach `asleep`; the existing 15-minute absence rule
   still applies. A separate explicitly authorized short test may use the
   existing minimum `nobody_us` of 60,000,000 and a night range containing now,
   but restore the original settings and identify that test separately.
4. Record the same five topics while First observes an unsupported name event
   and a naturally spoken name. A score alone is not evidence of a person.
   With no accepted speaking Activity within +/-2 s of name onset, expect
   continued sleep and exactly one `ignored sleeping name` line (coalesced
   count for a burst). With accepted person speech, expect `waking` on the
   next planner tick after corroboration, followed by the unchanged 2.7 s
   stretch at energy 0.5. Compare source timestamps, delivery times and the
   physical response separately; a state label is not a servo measurement.
5. Retain the night journal and a bounded trace around each unexpected wake.
   Distinguish name wake from unchanged touch, reminder, morning-face or
   Thinking/Speaking wake paths. First's existing body night journal can
   explain independent movement while Expression remains asleep. Report
   naturally spoken names that fail to wake, not just false wakes avoided.

Most likely wrong on the lamp: servo/room audio may also make Voice's detector
report person speech, which this gate cannot distinguish; or a true spoken
name may arrive without accepted Activity because of attention/readiness,
owner delivery loss, source-age bounds or Name publication timing. Activity
is an owner observation with 80 ms heartbeat and 120 ms speech hold, not raw
capture acquisition. Correlation tolerates a 300 ms delivery bound without
widening the +/-2 s source window. The existing legacy-topic wall-age adapter
and the new onset freshness check must be checked against real Name events.
Very dense accepted activity can exhaust the bounded 128-observation history;
oldest evidence then drops safely. Synchronous journal I/O can delay later
service steps even though current actuator requests publish first. None of
these acoustic, scheduling, privacy, thermal or physical response boundaries
was measured by Fourth on a lamp.

Rollback restores both the previous binary and its original catalogue. The
old strict parser does not accept the new field even if its value is false.
No detector, Voice, Body, deployment, default catalogue, key or account was
changed in this task. First owns any later default-on change after acceptance.
