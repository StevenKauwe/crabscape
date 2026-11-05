# Retain validated guest checkpoint bytes

Guest installation previously decoded and validated the received checkpoint,
then encoded the restored kernel again to create its recovery journal. The
journal can retain the received bytes after the same validating decoder succeeds.
A new constructor returns the validated kernel and journal together; the caller
selects authority from the immutable validated kernel. Malformed input fails
before that selection, and the browser still checks that the local visitor is
present before installing anything. Formats, simulation and saves do not change.
The internal metadata constructor is private, so callers cannot pair arbitrary
bytes with unrelated kernel metadata.

## Validation and measurements

The focused journal regression checks equivalent restored state, malformed-input
rejection before authority selection, independent transfer bytes, and exact
recovery after subsequent input history is serialized and reloaded. Existing
compaction checks remain intact. Wasm game clippy and formatting pass.

The same hardware-Chrome cells-session route as the host reuse experiment was
run on an optimized Binaryen 133 -Oz build, with fresh isolated profiles and no
heavy concurrent compilation. Both native regional replays passed.

| Metric | Host reuse only (#503 candidate) | Guest reuse added |
| --- | ---: | ---: |
| Host collection | 5.0 ms | 5.2 ms |
| Host grant creation | 33.0 ms | 33.8 ms |
| Guest installation | 26.0 ms | 11.1 ms |
| Host simulation pause | 149.9 ms | 166.7 ms |
| Guest simulation pause | 166.7 ms | 166.6 ms |

Neither run reported a frame gap above the profiler's 60 ms threshold. This is
one local comparison; scheduling varies at frame granularity and overall guest
pause did not improve measurably. The result removes redundant CPU work, not the
protocol wait or all transition stutter. No phone-performance claim is made.

Local logs: /tmp/crab-split-reuse-profile.log (baseline) and
/tmp/crab-guest-reuse-profile.log (candidate), with crabscape-15742 and
crabscape-20580 frame captures in the system temporary directory. Claude's
unfinished cellular-performance work remains untouched.
