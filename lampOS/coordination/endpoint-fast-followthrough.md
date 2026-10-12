# Endpoint workstream: finish the opt-in fast conversation path

Owner: Agent 2. Main has accepted the reviewed endpoint pilot for composition;
its default remains silence and its 1000/1200/1500 ms policy stays intact.
The immediate native priority is the independent MIC1 echo-reference fix.

The next complete package is a separate opt-in fast endpoint proposal, reusing
the earlier continuation candidate rather than replacing the worker or model.
Base the proposal on current `lamp-v2-chat` plus the reviewed endpoint pilot
through `ad0b2e329`. Keep shared coordinator/options changes in a separate commit.

Target: overlap at least 500 ms of the existing 1.2 s silence confirmation with
provider work on an unambiguously finished utterance. Keep the first audible
reply withheld until the existing confirmation boundary; resumed speech must
retire the unspoken answer and retain all of the new utterance. Use the current
coordinated kept-start/continue-or-abandon contract, including its finite ledger
and real capture timestamps. Do not relax a lease, trust an unconfirmed barrier,
replay an input, or change default behavior. An unsupported combination must be
refused before starting a process.

Deliver code through the real conversation loop and coordinated provider worker,
using a scripted service. Cover ordinary completion, breath/continuation, repeated
continuation, topic change, late output, model timeout/crash/missing model, provider
failure, Stop and mute while the answer is held. Retain negative controls for a
removed hold and stale verdict. Reuse the held-out human clip manifest and report
false cuts, intact thoughts, dropped prefixes and latency distributions against
the present pilot with no tuning on evaluation clips. Separate current strict
kept-start behavior from the historical immediate-Start failures.

The core must stay independent of inference. No new model, dependency expansion,
background job framework or deployment is required. Run focused formatting,
Clippy and tests on M2; do not run the office VM or play audio. Publish tested
source identities, exact boundaries and remaining physical/service assumptions.
Main reviews, composes and schedules actual service and device trials. A simulated
latency win is a candidate result, never evidence of a real-user experience.
