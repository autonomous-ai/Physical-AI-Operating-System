# Surround failures retained

All executions here used the development Mac, x86_64, Intel i7-10700K,
64 GiB RAM, macOS 26.6.2 and the workspace's Rust 1.96.0 toolchain.
This is a curated ledger of observed runs and review findings. File-backed
outputs are included where available; other counts came from the retained
tool transcripts and are not reconstructed raw logs. Captured text files have
trailing whitespace and extra terminal blank lines removed; assertions and
measurements are unchanged.

## Captured failures

| Run | Result and correction |
|---|---|
| Initial atomic tests | 2 passed, 7 failed at strict private-directory validation. The fixture's temporary root had mode 0755; fixtures now explicitly create 0700 roots. Production validation was retained. See `atomic-tests.txt`. |
| Initial reset tests | 20 failed at the same temporary-root permission check. The fixture was corrected to 0700; subsequent reset suites passed. |
| Initial release suite | 18 passed, 2 failed. `metadata_and_target_byte_caps_are_independent_of_signed_lengths` used metadata smaller than its alleged failing cap; it now signs a padded fixture exceeding the cap. `unknown_backwards_and_disagreeing_utc_refuse` compared against host-now-minus-one rather than the persisted floor; it now tests floor-minus-one. |
| TLS dependency integration | `adding_surround_preserves_the_existing_voices_implicit_tls_provider` failed because both AWS-LC and Ring Rustls features were active. The source-pinned Tough manifest disables Rustls defaults; its crypto source is unchanged. Explicit Ring HTTPS and the existing implicit provider regression now pass. See `tls-provider-before.txt`. |
| Intermediate HTTPS compile | Direct `rustls_platform_verifier` dependency was missing during integration. Added the dependency; no certificate verification was bypassed. See `tls-provider-after.txt`. |
| Operator strict JSON | 10 passed, 1 failed: `malformed_oversized_and_unknown_requests_are_redacted`. Serde's unit command variant accepted unknown fields, so a malformed body reached the worker and returned 503 instead of 400. Empty struct variants now reject unknown fields. See `operator-tests.txt`. |
| Initial worker socket tests | 13 passed, 4 failed with sandbox EPERM at `UnixListener::bind`, before assertions. The same 17 passed with permission for private local sockets. The four were Wi-Fi commit, qualified configuration application, original Voice observation age, and rejection of unproved configuration activation. |
| Worker periodic Voice proof | 20 passed, 1 failed: `probe_retains_original_voice_observation_age` expected `Some(99_000_000)` but got None. A setup attempt rejecting an older observation incorrectly erased valid global evidence. The fix withdraws only that attempt's freshness and preserves the original global acquisition time. |
| Worker preemption timing | Default-parallel run: 24 passed, 1 failed, 23.31 s. The initial 500 ms assertion observed 1,605,898 µs during concurrent repeated-fsync work. The isolated test observed 330,900 µs. Preemption worked in both cases: the pending probe never completed. The functional assertion now allows a generous ten seconds and proves the blocked probe stays blocked; measured latency is recorded separately, without a tight timing assertion. This contention result is not a lamp measurement. |
| Quiet-reset fixture | `voice_prepare_can_wait_for_quiet_but_retries_keep_the_original_deadline_and_other_actions_stay_short` omitted the fixture owner's freeze before acknowledging it. Added that scripted effect; the focused case and full reset suite passed. |
| Final root-rotation recovery | Release suite: 30 passed, 1 failed, 42.67 s. `root_rotation_is_retained_when_later_metadata_fails` rejected the repaired feed as corrupt because a second metadata capture re-imported cache data seeded under a retired root. An atomic `cache_metadata_captured` phase now commits with the first root/role watermark update; later target reads recover only UTC progress. The corrected release suite passed all 31 cases in 45.95 s. See `root-rotation-regression-failure.txt`. |
| Strengthened signed-target fixture | Release rerun: 30 passed, 1 failed, 46.66 s. `persistent_checksum_cannot_replace_signed_manifest_authentication` changed the claimed set hash inside signed targets too, correctly triggering signature rejection before the newly asserted target-hash check. The fixture now changes only the envelope's accepted-map key/highest-set pointer and recomputes its checksum, leaving signed bytes intact. See `signed-target-fixture-failure.txt`. |

Initial Clippy findings included `collapsible_if`, `manual_contains`,
`nonminimal_bool`, two `field_reassign_with_default` cases, and a test
`await_holding_lock` despite an explicit drop. Expressions/initializers were
corrected and the lock uses a lexical scope. No lint was suppressed. Temporary
parallel integration compiles also encountered missing module exports, web
assets and setup freshness methods while those files were being written.
An initial fixture inferred `Option<str>` instead of `Option<String>`;
an import used private `tough::schema::Key` instead of its public module path.

## Audit findings covered by regression tests

These are source-review findings, not invented failed pre-fix test runs:

- A 30-second owner deadline could not accumulate 45 seconds of quiet.
  Quiet-related actions now have bounded 120-second exceptions; retries retain
  the original absolute deadline. Ordinary actions remain 30 seconds.
- Tough advances its UTC datastore during target reads. The pending marker
  now survives those reads, and interrupted UTC progress is retained.
- Root-cache reconstruction, key revocation/rotation, overflowing versions,
  changed timestamp authority and authenticated accepted-target recovery are
  explicitly tested. A checksum is not treated as a signature.
- Failed manual rollback incorrectly counted recovery to the original set as
  completion of the requested change. It now remains a failed request.
- Cached Listening could survive known teardown until its remaining lease
  expired. Admitted lifecycle work now immediately withdraws that proof.
- Idle status previously did not continuously requalify readiness; failed
  probes and stale browser leases now withdraw it without refreshing old data.
- Repeated operation IDs had a lifetime 128-command cap, and configuration/
  release sets could exhaust retention. Bounded recent history and collection
  preserve current/previous/candidate references and trust floors.
- Reset RPC IDs could repeat across stores; declined reset replies could leave
  a non-resumable pending state. IDs are distinct and uncertainty is durable.
- Whitespace instructions, damaged-current rollback, bus/storage path overlap,
  request/connection bounds, stale HTTP session responses and worker admission
  ownership each have focused coverage.
- Network profile persistence followed by failed checkpoint finalization is
  `CommitUncertain`, not rollback success. Cross-attempt probes/recovery do not
  tear down a healthy selected LAN.

The real browser check found `[object Object]` in the Software panel. All seven
actual Rust outcome shapes now map to customer text; the Node regression and
Chrome recheck passed. The page's release placeholder was also corrected from
a path to the plain filename accepted by the release verifier. Desktop and
390×844 views, login, disabled controls, corrected status and logout were
observed using a disposable disabled local fixture. The in-app browser was
unavailable; Chrome supplied the check. No lamp or provider was connected.

## Not shown

No physical Wi-Fi, flash power-cut, servo rest, Voice cleanup, provider session,
production signing feed or customer trial was performed. Private socket tests
are local processes; foreign-UID refusal uses a changed expected UID, not a
privileged cross-user experiment. Authored fault times are not hardware times.
The final gate and source-copy results are reported separately in `results.json`.
