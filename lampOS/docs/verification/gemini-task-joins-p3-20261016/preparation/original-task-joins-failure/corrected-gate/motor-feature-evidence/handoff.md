Native-head first failure — source and configuration evidence

Recorded from date: 2026-10-16 09:47:10 +0700.

Exact identity: lamp-voice / coordinator::motor::tests::native_head::native_head_six_explicit_actions_prepare_profile_then_new_requests.
The unchanged assertion at native_head.rs:172 reported `phase Inhibited, failure
Some("motor worker progress timed out")`. The original 400 ms host assertion
bound is at line 162; the diagnostic originates in the unchanged body runtime
supervisor at line 429. The raw trace does not identify which action timed out,
the child's last successful operation, or a scheduler cause. No causal claim or
known-failure exception is made here.

The fixture's `profile` mode runs the original Controller, Driver, OptionalMotor,
real inert worker child and Unix channels. prepare_fixture at tests.rs:410 writes
the original script invoking this same test executable's inert_child identity.
No Gemini session or TaskJoins factory is called in that fixture path.

The exact executed Voice test binary is `lamp_voice-776bb3f8630d2566`. Its fingerprint has no
Voice features and links the Gemini non-test library fingerprint
`lamp-gemini-a51afd4ee6b5e2f8` with only the `default` feature. `fixture-clock` is absent.
Thus Gemini's cfg(test)/fixture-clock retained task adapters are not compiled
into this dependency. The separate Gemini unit binary did compile and run the
18 new tests. This does not convert the Motor failure into a passing gate.

source-feature-graph-proof.json records exact source, manifest, metadata, graph,
binary and matched fingerprint hashes. Snapshot copies preserve the inspected
inputs. ../completion-supplement/native-head-failure-excerpt.txt is the unedited
first failure block; ../full.log is the complete original run. No tests were
rerun and no repository source was changed.
