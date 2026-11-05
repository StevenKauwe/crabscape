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

This runs the existing `cells-session` travel fixture with a 70-second capture,
software rendering and Chrome CPU throttling at 4×. It requires both frame traces,
explicit confirmation of both native replay comparisons, more than 7,200 observed
guest ticks in the same shared region, and at least two guest compaction spans.
The profiler retains only its latest 512 work spans; fewer than two retained
compaction spans is inconclusive, not proof that compaction never ran. The
scenario remains bounded by the existing driver waits. An insufficient tick
span or a changed lease fails as an inconclusive reproduction rather than silently
claiming the exhaustion case passed.

This is a stress regression, not a calibrated FPS or millisecond performance gate.
Chrome CPU throttling is not a phone emulator. Current traces do not expose queue
length, so successful execution alone does not prove a continuously nonempty
backlog; the native queued-input test and the observed failing browser negative
control supply complementary evidence. Keep raw traces, build provenance and
renderer settings when comparing runs, and avoid concurrent heavy builds.
