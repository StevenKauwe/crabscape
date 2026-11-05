# Compact an applied follower journal while future inputs are queued

A shared follower appends a frame to its journal only after applying it to its
installed simulation. Received future frames remain in a separate queue. Waiting
for that queue to empty before compaction can starve a consistently lagging
follower: its journal eventually reaches the unchanged 7,200-tick bound and the
next append fails with `InvalidFrame`.

Compact the applied prefix at the existing 3,600-tick threshold, retaining the
Running-state and empty local partial-batch checks. Queued frames and future
boundary events are left in place. Existing checkpoint validation still requires
matching authority, tick, roster and revisions; no validation or history limit is
relaxed. This addresses journal exhaustion, not every source of transition stutter.

The native regression maintains 32 queued future inputs through 7,320 applied
ticks, compacts twice, and verifies unchanged future inputs plus exact journal
reload and replay. Its un-compacted negative control reaches `InvalidFrame` at
the original limit. The test models prefix correctness; browser scheduling needs
a separate check to catch starvation in the presentation-side compaction gate.

## Opt-in slow-browser regression

```sh
CRAB_FOLLOWER_WEB=/absolute/path/to/candidate/web \
CRAB_FOLLOWER_INTRODUCER=/absolute/path/to/introducer \
cargo test -p cooptest --test follower_journal -- --ignored --nocapture
```

This runs the existing `cells-session` fixture with a 70-second idle capture,
software rendering and Chrome CPU throttling at 4×. Leaving both crabs idle keeps
them in the same shared lease. The existing driver then checks matching host and
guest full state, native replay, report UI, and disconnect/rejoin behavior.

The stress test requires both frame traces, explicit confirmation of matching
host/guest state and its native replay, more than 7,200 observed guest ticks in the
same shared region, and at least one `cell-journal-compact-queued` span on the guest.
That nested opt-in timing span is emitted only when future inputs remain queued
at compaction, proving the browser executed the path previously forbidden by the
empty-queue prerequisite. It does not imply the queue stayed nonempty throughout
the entire capture.

The profiler retains only its latest 512 work spans; a missing queued-compaction
span is inconclusive, not proof that compaction never ran. The scenario remains
bounded by the existing driver waits. An insufficient tick span or a changed lease
also fails as an inconclusive reproduction rather than silently claiming the
exhaustion case passed.

This is a stress regression, not a calibrated FPS or millisecond performance gate.
Chrome CPU throttling is not a phone emulator. The native queued-input test and
browser observation supply complementary evidence. Keep raw traces, build
provenance and renderer settings when comparing runs, and avoid concurrent heavy
builds. The normal post-capture acceptance path exercises additional UI and
connection behavior, which can expose separate slow-browser failures.
