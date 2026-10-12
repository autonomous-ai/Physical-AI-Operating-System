# Native interruption diagnostic, 12 October 2026

The cached coffee interruption was **not admitted while Lamp was speaking**.
Both stimuli played completely. The first answer continued; a much later input
caused a second reply, but its source is unassigned. This is a failed interaction
case, despite successful diagnostic collection and cleanup.

The same `b3186e7e…73f4` runtime ran for 70 seconds with duplex turn taking,
capture-clocked reference, recovery off, kept-start continuation, NS off and a
200 ms playout horizon. The first source asked about sunset. The second, played
after the first speaker-write cue, asked to change the subject to coffee. Both
reuse cached Samantha/205 speech. Sunset's exact-zero tail was shortened into
a separately hashed diagnostic asset; no speech was regenerated. The isolated
sunset question does not meet the original scene's sky-context precondition.

The coffee source's first copy occurred 319.837 ms after the Mac received the
reply-start cue. These are host timestamps; physical overlap requires reviewing
the room recording. The runtime first yielded at capture frame 1113, then
rejected candidate 2. The original answer completed with two reported playback
gaps. Later candidate 3 was admitted; that answer had one reported gap. Its
source is unknown and it is not scored as a successful coffee response.

The complete 100-second iMac recording is valid, with zero reported drops,
clipping, stream faults or timestamp discontinuities. Both native raw/cancelled
microphone and accepted-render streams have valid acknowledged diagnostic
certificates and complete hashes. [Exact results](results.json) retain original
source identities and device events. Audio and receipts remain local in ignored
`.cache/physical-audio/20261012/duplex-diagnostic-fhld3dr3` for replay. No human
listening or acoustic score is claimed. The runtime exited normally, and original
mixers, stationary muted legacy services and hardware-owner release were verified.

Agent 1 is analyzing these signals to locate where the interruption was lost.
This run does not qualify natural interruptions or a general speech/noise rule.
