# The lamp's memory

`lamp-memory` (`crates/memory`) keeps what the lamp remembers from one day to
the next: a journal of every exchange it took part in (what it was asked, as
the provider wrote it down, what it answered, and when) and the things it was
asked to keep ("remember that I take my coffee black"). It is decision 12 of
[brain-build.md](brain-build.md). Memory is one person's: until the lamp can
tell people apart everything is under the person id `everyone`, and no request
gives one person's memory in answer to another's.

**What exists and what we take.** Keep the raw journal as evidence, use
SQLite FTS5 for ranked keyword retrieval, and let a separate model propose
small observations with explicit source links. The studies behind that
choice are recorded below. The implementation and public retrieval
comparison are in [memory-days.md](memory-days.md) and
[memory-evaluation.md](memory-evaluation.md).

**Two modes.** The JSONL mode described first below remains the default.
`--memory-days` enables the new store and request extensions. It has host
tests and measurements, but has not run on a lamp. No voice caller or
deployment is changed by this task, and no real model is configured.

## The process

Its own process, so that the lamp talks on, without recalling earlier days,
when this one has stopped, hung or crashed. Others ask over a Unix stream
socket and never read its files.

    lamp-memory --dir ABSOLUTE_DIR --socket ABSOLUTE_SOCKET_PATH [--keep-days N] [--utc-offset-minutes M] [--memory-days]

`--dir` is made with mode 0700 if missing, `--socket` with mode 0600, and a
socket file nobody answers on is replaced. A relative path, an unknown option
or a `--dir` its group or everyone can enter exits with 2 and says why; an
answered socket or a `--dir` another `lamp-memory` has open exits with 1. Once
the store is open a file `ready`, holding `ready` and a newline, is kept
beside the socket.

One connection is served at a time. A client has two seconds to send a whole
request line and two to take its answer, or it is dropped; a silent open
connection makes the next client wait, so close when you have your answer.
Each request is logged in one line (op, person, outcome, counts), never with a
word of what was said or kept. The process registers no signal handler:
SIGTERM ends it where it stands. A successful write is synced before its
request is answered, but the socket and `ready` stay until the next start
clears them or systemd removes the runtime directory.

## On the disk

This layout is the default. Enabling `--memory-days` imports it into one
private SQLite database, then removes the imported files before serving.
The conversion is one way: later starts on that directory require the flag.
Read the [migration and durability contract](memory-days.md#storage-and-migration)
before using an existing store.

    <dir>/<person>/journal-YYYY-MM-DD.jsonl   {"id":1,"at_unix_ms":1791892800000,"heard":"...","said":"..."}
    <dir>/<person>/kept.jsonl                 {"id":2,"at_unix_ms":1791892800000,"text":"..."}

Directories are 0700, files 0600. A person id is `[a-z0-9_-]{1,32}`; anything
else is refused before it is near a path. The day is the local day of
`at_unix_ms`, `--utc-offset-minutes` east of UTC (default 0; 420 for UTC+7).
Ids rise for each person across both files, and after a restart go on from the
highest on the disk. A line is added with one `write` and `sync_data`; a
removal writes the file anew beside the old one and renames it into place. A
line that does not read as an entry, such as one a power cut stopped in, is
skipped and counted (`skipped_lines`), and the rest of the file is used.

## Requests

One JSON object on a line, answered by one on a line: `"ok":true` and the
result, or `"ok":false`, an `"error"` (`bad_json`, `bad_request`, `unknown_op`,
`too_long`, `bad_person`, `bad_day`, `bad_time`, `empty`, `storage`) and a
`"message"`. A refusal never ends the connection. Each takes an optional
`"person"`, `everyone` if left out.

    {"op":"append","at_unix_ms":1791892800000,"heard":"How do I take my coffee?","said":"Black."}
    {"op":"remember","person":"mai","at_unix_ms":1791892800000,"text":"takes her coffee black"}
    {"op":"kept","max_bytes":2000}
    {"op":"recall","query":"coffee","day":null,"limit":8,"max_bytes":4096}
    {"op":"forget_last","kind":"kept"}
    {"op":"forget_all","person":null}
    {"op":"status"}

- `append`, `remember`: `{"ok":true,"id":3}`. `heard` or `said` may be left out.
- `kept`: `entries` of `{id, at_unix_ms, text}`, the newest that fit in
  `max_bytes` of text, oldest first.
- `recall`: `entries` of `{"kind":"exchange", id, at_unix_ms, heard, said}` or
  `{"kind":"kept", id, at_unix_ms, text}`, oldest first. With `day`
  (`YYYY-MM-DD`): that local day's exchanges. With `query`: what in the kept
  list and the journal has any of its words as a whole word, whatever the
  case; more of the words rank first, then the newer. With both: that day's
  exchanges that match. With neither: the newest exchanges.
- `forget_last` removes the entry with the person's highest id, of either kind:
  `{"ok":true,"removed":{"kind":"kept","id":2,"at_unix_ms":1791892800000}}` or
  `"removed":null`. Which entry, never its text. With `"kind"` (`kept` or
  `exchange`) it is the highest of that kind, whatever came after it:
  "forget what I asked you to remember" means the last kept thing. Any
  other `kind` is `bad_request`.
- `forget_all` empties one person's directory, unreadable lines included:
  `{"ok":true,"removed":14}`. With `"person":null`, everyone's; every other
  request but `status` refuses `null`.
- `status`: `people`, each `{person, exchanges, kept, bytes, oldest_day,
  newest_day, skipped_lines}`, everyone's unless one is named.

### Additional requests with `--memory-days`

The old request and response shapes remain accepted. `recall` now selects
by FTS5 rank and still returns the selected raw entries in chronological
order; it does not return observations. New operations are:

```json
{"op":"search","person":"everyone","query":"cà phê","limit":8,"max_bytes":4096,"history":false}
{"op":"append_linked","at_unix_ms":1791892800000,"heard":"My coffee?","said":"Black.","source_ids":[2]}
{"op":"forget","id":2}
{"op":"session_open","session":"voice_1","max_bytes":1200}
{"op":"session_close","session":"voice_1"}
{"op":"conversation_end","conversation":"turn_1","source_ids":[1],"previous_ids":[]}
{"op":"tidy_apply","job":"j1","observations":[{"text":"Prefers black coffee","category":"preference","source_ids":[1],"supersedes":null}]}
```

These are shapes, not an ordered script: IDs must come from successful
responses in the same person's store. `search` returns ranked records with
their provenance; `forget` returns a count after removal and index checks.
`session_open` returns a frozen `snapshot`; `conversation_end` returns
bounded model `input`, without invoking a model; `tidy_apply` returns the
new observation `ids`. See [the full wire contract](memory-days.md#request-contract)
for fields, errors, retries, invalidation and caller responsibilities.

## Limits, and what happens at each

- A request line over 64 KiB: `too_long`; the next line is read as usual.
- `heard` 600 bytes, `said` 800, `text` 300: cut at a character, never refused.
  Whitespace and control characters become single spaces first; an entry with
  nothing left is refused (`empty`).
- 200 kept things for a person: the oldest is dropped as the next arrives.
  With `--memory-days`, the next write is refused as `full` instead;
  explicit memories are never silently evicted.
- `--keep-days N` (default 30): a journal day is deleted once it is more than
  N days before today by the lamp's clock, at start and at the first request
  of a new local day. Kept things have no age. A clock set far ahead empties
  the journal.
- `recall`: `limit` 8 unless given, at most 50; `max_bytes` 4096 unless given,
  at most 16384; more is lowered, not refused. Bytes are of text; the first
  entry that does not fit whole ends the answer. A query's words are those of
  two characters or more, the first 16. `kept`: 4096 bytes unless given.
- `at_unix_ms` outside the years 1970 to 9999: `bad_time`.

## How the voice uses it

`crates/voice/src/remember.rs`, switched on by naming the socket in the
voice unit (`LAMP_MEMORY_SOCKET`, the provider worker's `--memory-socket`).
Without it the voice knows nothing of this process.

- **Three functions the model may call** ([tools.md](tools.md)):
  `remember {text}`, `recall {about?, days_ago?}` and `forget {}`. Each is
  one request here, on a thread of its own, waited for 1.5 s at most. With
  no answer the model is told "My memory is not answering right now, so I
  could not do that." and the conversation goes on.
- **The journal.** When a request is over, the exchange (what the provider
  wrote down of the person, and of the lamp) is queued for one writer
  thread, which sends `append`. The queue holds 32; a full queue or a
  memory that does not answer loses that exchange and nothing else. An
  answer with no word in it is not journalled. Talk the lamp did not
  answer never reaches here.
- **Asking it to keep or forget something is not itself kept.** The
  exchange in which `remember` or `forget` was called stays out of the
  journal: the kept list is the record of that.
- **A note for each new session.** Every session is told that it has a
  memory and when to consult it ("when someone asks about an earlier
  conversation, or about something they told you before that you do not
  see here, call recall ... If it gives nothing, say you do not
  remember"), and then the newest kept things (1,200 bytes of text), with
  the rule to use one only when the person brings its subject up, never to
  list them, and never to say "you told me": the lamp cannot tell who is
  speaking. The kept things are read once when the voice starts, waiting
  300 ms at most, and after that by the writer thread: after each
  `remember` and `forget`, and every five minutes. Opening a session
  never waits for this process.
- **What the model is given from a recall** is how long ago in words
  ("yesterday", "2 weeks ago"), never a date or a time of day: a person
  does not say "on 3 October at 14:02".
- **`forget` is not yet all of forgetting.** It removes the last kept
  thing, clears the note and the conversation thread, and that is what the
  lamp says ("it is off my list"). A later conversation in which the thing
  came up is still in the journal until it is 30 days old. Forgetting that
  reaches everything derived from a thing needs each entry to say what it
  came from. The opt-in store now supports those links and transitive
  deletion; the voice does not yet send them or use the new snapshots.

### Played on the lamp

On lamp-4ace on 2026-10-13, late in the evening with nobody in the office,
a synthetic voice through a loudspeaker, once each
(`lamp-memory.service`, its own unit and user; the voice at `7da2571cf`):

| Said to the lamp | What it did | First sound after the end of speech was decided |
|---|---|---|
| "Hey Lamp, please remember that I take my coffee black." | Called `remember` (the memory answered in 10 ms) and said "Got it. Black coffee." | 1.59 s |
| "Lamp, how do I take my coffee?", in the same conversation | Called `recall` and said "You take it black." | 1.95 s |
| The same question to a new session that had never heard it said, the conversation thread removed | "Black, just the way I like it too.", from the note, with no call | 1.32 s |
| "Lamp, what did we talk about earlier today?" | Called `recall` (six entries) and said "We chatted about a math question, and you also asked about how you take your coffee." | 2.09 s |
| "Hey Lamp, forget what I asked you to remember." | Called `forget` (6 ms) and said "It's off my list." | 2.25 s |
| The coffee question to a new session after that | Nothing: the service heard a statement ("I don't want to take my coffee.") and did not answer | |

On the lamp the memory answered `append` in about 10 ms and `recall`,
`kept` in under 1 ms, with a journal of a few dozen lines. A reply that
uses a function begins roughly a second later than one that does not.

The run before it, on the first build (`bfa8f57a9`), is why two things
are as they are. A new session did not use the kept thing: the voice had
opened its first session a moment before the writer thread had read what
was kept, so what is kept is now read before the first session, and a
question about the coffee to that session was declined as not for the
lamp. And "what did we talk about earlier today?" got "I don't remember
earlier conversations. We're just starting fresh!" with no call, which is
why every session is now told that it has a memory.

Not shown by this: whether a thing that was forgotten still comes back
from the journal (the last row was not answered at all), anything with a
person and not a loudspeaker, a second language, more than one thing
kept, a journal of more than a day, or what happens to the lamp's
ordinary answers with three more functions declared.

## What is taken from how others do memory

The owner's question on 2026-10-13: is rolling our own right, or should
the lamp use what the field has worked out? Three studies were made that
evening and are kept outside the repository, each claim with its source:
how agent products do memory (Claude Code, Codex, ChatGPT and others),
the open-source memory systems (mem0, Letta, Zep and Graphiti and a dozen
more) with their benchmarks and the disputes over them, and what a spoken
companion in a shared room needs that a chat agent does not. Their
findings are the studies', read from the sources named there and not
re-measured here.

The answer they support: **adopt none of them whole, and build on the
journal.** No open-source system fits as it is: each is a Python service
with a vector or graph store and a model call on every write, and in
independent tests the well-known ones do not beat plain keyword search
over the raw record. The products people use every day converged on
something simpler: readable text, a small part always in the prompt under
a hard cap, the rest searched on demand, and a pass in the background
that tidies up. The journal here is already the layer the evidence
supports most. **Except where the last column says so, this is a decision
record and not what the crate does.**

| What the best of them do, and the evidence | What the lamp's memory does about it | In the crate |
|---|---|---|
| Keep the raw record and search it; treat extracted facts as an index over it, never a replacement (the best-supported finding: replacing sessions with summaries lowered accuracy in LongMemEval) | The journal is the truth. Anything derived points back to the lines it came from and can be rebuilt | Opt-in raw records plus validated observation source links; scripted tests |
| A fast path for "remember this" | The kept list, written at once | Yes |
| A small part always in the prompt, under a hard cap, as it stood when the session opened | Open promises, recent threads, lasting preferences | Opt-in frozen snapshot, at most 1,200 UTF-8 bytes including its JSON framing; voice integration pending |
| Tidy up in the background, never during a reply (Codex, ChatGPT, Letta, OpenClaw) | A pass after a conversation has ended, with a model off the voice path: dated observations, each with its source lines | Bounded prepare/apply protocol and cancellable model interface with scripted tests; real adapter absent |
| Stamp time on everything; a newer fact supersedes an older one by a link, and both are kept (overwriting and model-decided deletion were wrong often enough that mem0 gave them up) | Supersession as a link. Nothing derived is destroyed by the pass that made it | Opt-in source-dated observations; correction links preserve history; stale results refused |
| Keyword search first; meaning-based search only where it is measured to help | SQLite FTS5 ranking; no embeddings without evidence | Opt-in Unicode-normalized English/Vietnamese retrieval; LongMemEval user-session recall-all@5: 69.69% whole-word, 81.86% FTS5, 419 questions on the development Mac |
| Forgetting that is real: it reaches every summary and index (regulators acted against a product that kept transcripts of "deleted" recordings) | Remove the thing and linked derivatives, and verify the index before acknowledging | Opt-in transaction covers records, source/replacement links, FTS, pending jobs and stored notes; callers must drop already supplied model context |
| Age out what goes unused (Copilot 28 days, Codex 30) | Derived facts lapse when unused; the journal goes at 30 days; what a person asked to be kept has no age | Opt-in observations lapse after 30 unused days or when their evidence expires; explicit kept facts do not age |
| Scope by person, and never tell one person what another said (rated the most damaging failure for a shared room) | Until someone asks to be recognised, everything is "said in this room": used when its subject comes up, never attributed, never volunteered | All records, jobs, links and snapshots scoped by person; default is everyone; identity remains caller-supplied |
| Treat what is heard as untrusted, and check a fact before leaning on it (the recorded errors of a deployed voice agent's memory came from mishearing) | The lamp says back what it kept; names, times and promises need confirmation | Structured untrusted snapshot and strict output/provenance validation; no semantic truth check or new voice confirmation flow |
| Reminders are not memories: their own store, and a clock outside the model | Not part of this crate: a part of its own | No |
| A page where people see what is kept and can delete it | With the operator pages | No |

Not taken: a knowledge graph (it added 1.6 points for mem0 and costs a
model call or several for each write), a vector database as the store,
scores of importance and forgetting curves set by hand (no evidence they
help anyone), and any hosted memory service (every conversation in the
office would go to one more company).

**How it will be known whether this is good.** Three numbers, reported
together: accuracy, how long a read takes on the lamp, and how much text
leaves the lamp for a third party in a day. Accuracy on LongMemEval-S
(public, five abilities scored apart, and its retrieval score needs no
model) beside three baselines on the same harness: the whole
conversation in the prompt, today's whole-word match, and ranked keyword
search. And a set of the lamp's own: scripted conversations over several
days played to it, with questions whose right answer is "I don't
remember" or "that was someone else". The public retrieval comparison and
scripted multi-day store tests now run on the development Mac; see
[memory-evaluation.md](memory-evaluation.md). Full context is a retrieval
ceiling only, not a measured answer score. No daily provider traffic or
new lamp performance has been measured.

## What it deliberately does not do

No audio, no pictures. Nothing is sent anywhere from here, though a line
leaves the lamp, to the voice provider, whenever the voice uses one in an
answer. The default has whole-word matching; the opt-in mode normalizes
Unicode and ranks with FTS5, with no embeddings or semantic answer model.
Its observation pass has only a scripted adapter here. It does not know who speaks: the person is
whoever the caller says. Files are not encrypted; forgotten text is unlinked,
not scrubbed from the flash. The offset from UTC is fixed: no summer time.

## Not shown

Host tests on a development Mac: store and protocol in unit tests, the real
server on a socket in a thread, the built binary killed with SIGKILL and
started again. On 2026-10-13 the same 34 tests passed once as a Linux ARM64
build under emulation in a container; `forget_last` with a kind was added
after that run (36 tests on the host). The voice's side is host tests: the
three functions against a scripted memory, the journal's queue and writer,
and the provider worker with the scripted provider and a memory on a
socket. On a lamp: the played run above, once. A power cut was imitated by cutting
a file short. Moving the socket to its path after binding it has run on macOS and in
that emulated Linux run. Times, on that Mac through the socket with thirty days of 200 exchanges
(`tests/timing.rs`): `append` 19 ms, `recall` by words 15 ms, by day or newest
under 1 ms, `forget_last` 50 ms. None is known for the lamp.

The opt-in implementation's host results and current limitations are in
[memory-evaluation.md](memory-evaluation.md) and
[memory-days.md](memory-days.md#not-shown). The historical lamp and emulation
runs above do not validate SQLite, its native ARM64 build, the new protocol,
a real extraction model, or any changed voice behavior.
