# Continuation service probes, 2026-10-11

One control timed out; two opt-in cases settled the old response and answered
the corrected question. These are three separate real Gemini connections from
Lamp-4ace using cached synthetic PCM directly. No microphone, speaker, camera,
motor or running Lamp service was exercised or changed. Each configuration has
one observation; this is not a reliability rate or an acoustic benchmark.

The first question asked for the ocean's deepest point; the successor corrected
it to average depth. All cases used `Require`, a 2,000 ms barrier and opt-in
`AtCommit` accounting. The two candidate cases also used a 2,500 ms retained-Start
patience bound. The production default `FromInterruption` was not compared.

| Case | Outcome | Observation |
| --- | --- | --- |
| Immediate Start, 300 ms after first End was queued | Timeout, no audio | End was withheld 2,003 ms; total barrier lifetime was 6,182 ms. |
| Retained Start, same trigger | Confirmed and answered | Held Start 717 ms; retained 650 ms of PCM; released on earlier output. |
| Start after first received output, retention enabled | Confirmed and answered | Old response settled; holding was unnecessary in this case. |

The successful output transcripts addressed average depth: approximately
12,100 feet and 3,700 meters. Relevance was reviewed from text only; accuracy
and voice quality were not assessed. Successor End locally queued to first
successor audio received was **954.722 ms** and **959.549 ms**, excluding capture,
endpointing and speaker output. No wire acknowledgment establishes when End was
accepted by the service. Both passes completed an 8,001 ms aftermath with no
additional surfaced events.

The artifact is `08a80e49...555551`, from exact source `901622831`. It contains
the separately reproduced retained-cancelled-End defect. These cases never
cancel a held successor: they do not qualify that correction, the provider
worker integration, real-person interruptions or release enablement. The
717 ms hold is not a claimed latency saving.

All three owned child processes were reaped, their groups were gone, and no
forced kill or retry was needed. The wrapper verified unchanged provider and
credential identity metadata and selected legacy service states/PIDs. Main's
independent read-only audit reverified all 50 frozen helper files, 412 source
manifest entries, artifact and input hashes, event totals, stream hashes and
reported cleanup. The remote streams were not fetched a second time: embedded
JSON and source-defined stderr reproduce their recorded hashes.

[Sanitized results and complete limits](results.json) retain the exact source,
artifact, receipt and preparation identities. Raw recordings and credentials
are absent. The original private receipt SHA-256 is
`f2109d30a499adc58cdbabc9ed9020d607467f8df0a05a224d3d517ce51494f1`;
the independent audit report SHA-256 is
`d639dbfe5ea6c8e7706ac69accf425c4bc7d24ebaf560be84bb75f7c38f05005`.
