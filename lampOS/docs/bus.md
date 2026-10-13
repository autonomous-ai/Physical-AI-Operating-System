# The bus

`lamp-bus` (`crates/bus`) is how the lamp's processes tell each other what
they sense and what they make of it. A sensor publishes its stream once
(`mic1`, `mic2`, `camera`, `servos`), any number of consumers subscribe, and a
consumer publishes what it derives (`people`, `gemini/transcript`) the same
way. It is decision 15 and step 0 of [brain-build.md](brain-build.md).

There is no broker: the lamp is several processes with different owners, one
failing must take no other down, and a process in the middle is one they
would all need. A topic is a directory, a subscriber a datagram socket in it,
and a publisher sends each message straight to every socket it finds there.

## Layout

Under a root, `/run/lampos-bus` unless `LAMP_BUS_ROOT` or the caller names
another (an absolute path, on a filesystem with hard links):

    <root>/<topic>/subs/<subscriber>.<pid>.<count>.sock   one for each subscriber
    <root>/<topic>/latest                                 the newest message, kind `latest` only

Directories are made when first needed, mode 0770 unless the `Bus` is told
another; sockets and `latest` are 0660. A topic is segments of
`[a-z0-9][a-z0-9_-]{0,31}` joined by `/`, which nests directories, or by `.`,
which does not (`gemini/transcript`, `room.state`): 96 bytes at most, no
directory called `subs` or `latest`. A subscriber's or publisher's name is
`[a-z0-9_-]{1,32}`. Anything else is refused before the filesystem is touched,
and so is a socket path over 103 bytes, the less of what macOS and Linux hold.
A subscriber binds as `<...>.new` and links that to its `.sock` name, the
only names publishers look at: a socket's file is there a moment before the
socket can be sent to, and on the Mac 61 of 400 bound directly were removed
as dead by a publisher that looked with every message.

## A message

One datagram: a line of JSON, a line end, then the payload's bytes untouched.

    {"topic":"servos","kind":"latest","enc":"json","source":"lamp-body","boot":"9f3a6c1e04b7d25a","seq":412,"at_us":98123456,"at_unix_ms":1791878400123,"expires_ms":500}
    {"pitch":2048,"roll":2051,"yaw":1990}

`enc` says how to read the payload: `json`, `none` (there is none), or a
label for media such as `pcm_s16le_16000_mono`. `source` is the publisher's
name, `boot` sixteen hex characters it picked when it was made, `seq` its
count on this topic from 1, `at_us` the caller's monotonic clock in
microseconds, `at_unix_ms` the wall clock, `expires_ms` (optional) how long
the value means anything. The `kind` is one of two:

- `latest`: a value that replaces the one before it (where the servos are).
  The publisher sends it, then puts the same bytes in `latest`, written
  beside and renamed over, so a subscriber that starts late reads one whole
  current value at once (`Bus::latest`).
- `event`: something that happened and is over (a piece of sound, a word
  heard). Nothing is kept: whoever was not listening does not get it.

A whole message is at most 60,000 bytes; a larger one is refused to the
publisher, never cut and never sent. Sound goes as byte payloads in small
pieces (20 ms of 16 kHz mono is 640 bytes). A camera frame stays off the bus:
its topic publishes a small message naming a file the publisher keeps.

## What is promised, and what is not

A publisher never waits and never fails because of a subscriber. Each send is
non-blocking and ends one of three ways: taken; dropped and counted, because
that subscriber's queue is full; or refused because nobody holds the socket,
whose file is then removed. `publish` answers `{delivered, dropped, removed}`
and keeps totals, for each subscriber too. It is an error only for a message
too large or badly labelled, and once, not with every message, for a root it
cannot use or more than 64 subscribers. `subs/` is read at most every 200 ms
and when a subscriber is found dead; between reads, an event with nobody
subscribed costs no system call.

A subscriber is not promised delivery. It is promised order within one
publisher, none between two, and that loss shows: each message has `gap`, how
many of its publisher's were missed since the last one received, and
`publisher_restarted` when that publisher is a new instance whose count began
again. A new subscriber waits up to 200 ms to be found, and what was sent
before is not a gap. On a `latest` topic, `recv` with nothing waiting returns
the file's value if the subscriber has not had it, so the newest value
arrives though its datagram was dropped.

## The tool

    lamp-bus topics                                 # servos subscribers=2 latest=yes
    lamp-bus tap gemini/transcript --count 5        # header, then the payload if JSON, else <N bytes>
    lamp-bus latest servos                          # status 1 if the topic keeps none
    lamp-bus pub room.state latest '{"people":1}'   # as lamp-bus-cli, at_us 0 unless --at-us
    lamp-bus record /var/tmp/rec mic2 servos --seconds 60
    lamp-bus replay /var/tmp/rec/servos.jsonl --speed 2

Each takes `--root PATH`; bad input prints the usage and exits 2. `record`
appends `{"header":{...},"payload_b64":"..."}` lines to `DIR/<topic with / as
_>.jsonl` until killed or `--seconds` have passed, and stops a file at
`--max-mb` (64 MiB) saying so. It cannot catch a signal: SIGTERM ends it with
whole lines on disk and its sockets left for the next publish to remove.
`replay` publishes them again as `replay`, spaced by `at_us` over `--speed`.

## From another language

```python
import json, os, socket
name = f"/run/lampos-bus/servos/subs/py.{os.getpid()}.0"
os.makedirs(os.path.dirname(name), mode=0o770, exist_ok=True)
sock = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
sock.bind(name + ".new")                    # a name publishers pass over
os.chmod(name + ".new", 0o660)
os.rename(name + ".new", name + ".sock")    # its own, now that it can be sent to
try:
    while True:
        head, _, payload = sock.recv(60001).partition(b"\n")
        header = json.loads(head)
        print(header["source"], header["seq"], json.loads(payload) if header["enc"] == "json" else len(payload))
finally:
    os.unlink(name + ".sock")
```

## Not shown

Host tests on a development Mac (x86_64): the library through real sockets,
the built binary run and killed, the Python above against it. On 2026-10-13
the same 43 tests passed once as a Linux ARM64 build under emulation in a
container (no Python there, so not the Python part): that run is the first
in which a 60,000-byte message was delivered and a bound socket was linked
to a second name on Linux. Never run on a lamp, and nothing publishes on it
yet. Socket buffers are the system's defaults, which
the standard library cannot change. On this Mac a subscriber that does not
read held 25 messages of 141 bytes, or 5 pieces of sound, before the next was
dropped, and the system takes no datagram over 2,048 bytes: the test of a
60,000-byte message asserts delivery on Linux and that refusal here. Each subscriber is sent to from a socket of its own
because Linux charges a queued datagram to its sender (`unix(7)`), so one
socket for all would let one stuck subscriber starve the rest: that is read,
not measured, and macOS charges the receiver, so no test here tells the two
apart. How many messages a subscriber that does not read holds on the lamp
is not known.
