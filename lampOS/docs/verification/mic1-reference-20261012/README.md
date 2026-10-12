# MIC1 reference timing: integration provenance

This proposal composes exactly five Rust files from `16e864692aafd2b4efa7076a71c7e46dffd023fd` onto canonical
`7e28dc7ace66eb26cb3e3332f3dcadf02f1f5b9a`. The four non-test files are byte-identical to runtime `f18362fa947a8f90e4139a3536f04de8449ce2e6`,
used by native artifact `33f2eb2149bebebde7319c4ecd59a3b3c7470c43f88c2a9fbe56a70aa1589800`
(9,503,712 bytes). Main subsequently combined this exact MIC1 source with the
endpoint overlay and built/tested the frozen 518-file source. The resulting
ARM artifact is `7e5a7e015f9f0e682f6577e290d8375849421a167dd4bf0ad0b5ef69464f7ea7`;
its separate native checks exercised the endpoint model and IPC, not a live
conversation. [Combined evidence and retained failures](../endpoint-native-20261012/README.md)
pin the 45-path composition against the same canonical base. The MIC1 input
preserved canonical observer/evaluator/microphone-forwarding code; the endpoint
overlay makes its separately reviewed changes, and this follow-up updates docs.

## Runtime behavior and cost

Only `--microphone mic1 --reference-clock capture` holds four original 10 ms
capture blocks. Each new read advances the reference; the oldest retained frame
then goes through AEC, VAD and publication. Its PCM, sequence, privacy generation
and original read timestamps remain paired. MIC2 remains the default; MIC2 and
fixed-reference paths have zero hold. No new default, threshold or recovery
policy is introduced.

This adds about 40 ms of input age, not a wider lease. The coordinator still
rejects input aged 100 ms or more. Stop, microphone privacy closure/generation
change and capture-worker failure drop the pending frames. The last four reads
are intentionally not emitted as diagnostic pre/post pairs. Reference counters
describe processing time and lead the emitted source sequence by four before a
reset; join diagnostics by the original source sequence/read stamps.

## Evidence, with separate boundaries

The prior `f18362fa9` ARM/QEMU receipt records formatting, strict live Clippy,
release compilation and 21 focused tests passing. Its binary and receipt hashes
are in [results.json](results.json). That earlier receipt does not cover the
later `16e864692` assertions. The latter changes only
`crates/live/tests/capture_hold_worker.rs`: it checks explained-echo evidence for
the held synthetic signal and an unheld negative control, replacing attenuation
alone as the held-case assertion. The author found that suppression alone can
lower scripted noise without alignment.

The later combined-source ARM/QEMU gate passed all three capture-hold worker
tests, six reference-link tests, and eight reference-prime tests, plus format,
strict all-target Clippy and release compilation. This now covers those exact
assertions in the composed source. One endpoint IPC process test failed under
QEMU; the same test ELF passed 12/12 natively, with the failure/root-cause limit
retained in the [combined verification](../endpoint-native-20261012/README.md).
No complete ARM workspace pass or physical speech qualification is inferred.

The Linux tests call the production capture loop/AEC and IPC with scripted
microphone input and a fixed 30 ms synthetic echo. They include preliminary
warm-up sessions, so they do not establish cold-start reliability, physical
echo quality, human overlap, or held-frame behavior across every DSP reset.
The independent source review found no material blocker for the bounded native
trial, with these limits; it ran no tests.

Main also ran one fixed-source treatment on 4ace with the earlier `33f2…`
artifact. The result is
`diagnostic_collected_unscored`, with runtime exit 0, valid diagnostics and
verified rollback; 337,680 fixture PCM frames were accepted. The comparison
below is **reported by Agent 1**, preserved without rerunning its scripts.
The two runs used MIC1, V1-baseline turn taking, capture-clock reference,
recovery off, noise suppression off and unchanged writable mixers. They are
not Gemini conversations or human interruption tests.

| Reported quantity | Baseline | Four-block hold |
|---|---:|---:|
| Processed after own read, median | 1.023 ms | 40.511 ms |
| Processed after own read, p95 / maximum | 3.286 / 6.189 ms | 43.004 / 47.018 ms |
| Reference capture count minus emitted source sequence | 0 | 4 |
| Whole-reply level reduction | 2.8 dB | 24.7 dB |
| Lamp-alone copies 1 / 3, level reduction | 5.2 / 1.6 dB | 27.9 / 22.6 dB |
| Lamp-alone copies 1 / 3, VAD >= 0.8 blocks | 114/430; 193/360 | 0/430; 10/360 |
| Quieter coffee replay over Lamp, VAD >= 0.8 blocks | 136/337 | 0/332 |

Agent 1 reports the early Lamp sound processed two blocks before its reference
in the baseline and two blocks after in treatment. Its filter peaks of -16.8 ms
and +22.4 ms are relative to the recorded reference cursor, not calibrated
acoustic delays. One recording per condition cannot establish general
cancellation or speech-preservation quality. The coffee source remained much
quieter than Lamp and was not admitted; no source score, human-listened result,
acoustic latency, or MIC1 default promotion follows from this comparison.

## Retained provenance

Native numerical inputs are pinned in [inputs.sha256](inputs.sha256).
Original analysis: `/private/tmp/lamp-a1-mic1-timing-result-20261012T0704Z-rrcXJ4`; README SHA-256
`687f53cfa39bc8d77baab35fd8ec231188b088fdeeaf7eb10f65c7b2a64ce73f`.
Independent review: `/private/tmp/lamp-mic1-hold-review-11ze3jwy`.
The original analysis, numerical outputs, source review and exact analysis
scripts are copied to `/private/tmp/lamp-mic1-hold-integration-M3bwNoQf/private-provenance`; its sibling `provenance-files.json`
records every byte hash. Scripts are private analysis tools, not a runtime
dependency, and no Python/shell file, audio or executable is added to lampOS.

The original MIC1 export performed only byte/hash, JSON and patch checks;
[results.json](results.json) retains that historical provenance. The later
[combined verification](../endpoint-native-20261012/README.md) records the actual
composition gates, endpoint native checks, exact artifacts, failures and limits.

## Four recent MIC1 runs

The [four-run verification](four-runs.md) and [numerical summary](four-runs.json)
retain coffee alone, exact fixed-source timing, Gemini timing, and the failed
lower-speaker trial. Inputs were cached synthetic speech; none has a human
listening or acoustic score. All four 100-second room recordings and diagnostic
completion receipts verify, and all four rollbacks verified, independently of
conversation outcome.

The Gemini timing run missed the delivered coffee interruption and overlaps a
reported office-VM build from 07:12:12 to 07:13:39 UTC. Lower speaker admitted
three owners after two delivered sources; the helper aborted with SIGTERM after
two generated replies. Its source remains unclassified. Only speaker volume
changed 9,9 to 3,3; the journal and final readbacks verify restoration to 9,9
with microphone controls unchanged. Neither run is an interruption success or
a lower-speaker win.

The reported **endpoint-to-provider/output times exclude the preceding
1,200 ms silence wait**. They are not last-phoneme-to-first-audible-word
measurements. No conversation p50/p95, matched causal speedup, or MIC1 default
promotion is claimed.
