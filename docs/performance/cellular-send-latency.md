# Send cellular messages after simulation

Cellular input/reply processing flushed its reliable outgoing FIFO during the
input phase. Canonical batches and stop acknowledgements produced later by
simulation waited until the next rendered frame's input phase. The existing
post-simulation send system now flushes that same FIFO too. Input-phase replies
still flush immediately there; wire ordering, protocol, validation, authority
and simulation ticks are unchanged.

Both paths share the original packet/send behavior. A failed send ends the
session through the existing restoration path, records the failure and updates
status. The extra flush does not consume input or advance simulation.

## Validation

Wasm all-target game clippy, formatting and diff checks passed. The existing
hardware-Chrome cells-session walk used isolated profiles and optimized Binaryen
133 -Oz builds. Both independent regional replays matched native state.

| Metric | Guest checkpoint reuse baseline | Post-simulation flush |
| --- | ---: | ---: |
| Host simulation pause | 166.7 ms | 100.0 ms |
| Guest simulation pause | 166.6 ms | 133.4 ms |
| Host collection | 5.2 ms | 5.1 ms |
| Host grant creation | 33.8 ms | 33.7 ms |
| Guest installation | 11.1 ms | 11.3 ms |

Neither capture reported a rendered-frame gap above the profiler's 60 ms
threshold. Host ticks still advanced during Sending (1294 to 1312 in the
candidate), preserving the independent-host progress improvement. CPU work is
essentially unchanged; the result is consistent with reduced scheduling waits.
This is one paired local comparison, not a universal latency guarantee or a
phone benchmark. A substantial guest pause remains.

The separate cells-regions scenario also passed: actual-key separation beyond
1700 pixels, both players digging, both native replays, Home reunion with
identical world hashes, and private-world restoration after owner loss. Its
204 ms stage-based handoff measurement covers the whole protocol and is not
the same metric as the simulation pause above. No fixture assertions changed.

Local evidence: /tmp/crab-guest-reuse-profile.log (baseline),
/tmp/crab-send-latency-profile.log and /tmp/crab-send-latency-regions.log.
Candidate frame captures use crabscape-28787 in the system temporary directory.
Claude's unfinished canyon, tether, sound and cellular-compute work is untouched.
