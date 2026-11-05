# Reuse the initial regional checkpoint

A region grant previously encoded a checkpoint for transfer, then encoded the
same kernel again to initialize its recovery journal. Start the journal once
and copy its encoded bytes for the transfer. This applies to both shared and
separate grants. The independent byte buffer cannot mutate the journal, and
receivers still use the existing validating checkpoint decoder. No formats,
physics, terrain or save behavior change.

Opt-in User Timing spans now distinguish collection, grant creation and guest
installation: cell-split-collect, cell-split-grant and cell-split-install. They
use the existing data-cell-work-profile switch and do not enter simulation state.

## Measurement

Hardware Chrome on the same Mac, Binaryen 133 -Oz optimized builds, fresh isolated
profiles, existing cells-session 30-second walk with both native regional replay
checks. The baseline is b277a9b with only the three timing spans; the candidate
adds checkpoint reuse. No heavy compilation ran during either capture.

| Metric | Baseline | Reuse |
| --- | ---: | ---: |
| Host collection | 5.1 ms | 5.0 ms |
| Host region grant | 47.4 ms | 33.0 ms |
| Guest installation | 25.6 ms | 26.0 ms |
| Host simulation pause | 166.6 ms | 149.9 ms |
| Guest simulation pause | 183.4 ms | 166.7 ms |

Neither capture recorded a rendered-frame gap above the profiler's 60 ms
reporting threshold; both regional replays matched native state. These are one
paired local measurement, not a universal timing guarantee or a phone benchmark.
The improvement removes duplicate encoding; network scheduling, stop collection
and guest installation remain. The guest still initializes its journal through
the existing encoding path. Claude's unfinished cellular-compute work is untouched.

The focused journal test verifies checkpoint restoration, independent transfer
bytes and exact future journal recovery/compaction. Browser validation exercises
initial shared play and transition to separate regions with replay checks.

Local logs: /tmp/crab-split-timing-profile.log and
/tmp/crab-split-reuse-profile.log. Captures use crabscape-13911 and crabscape-15742
respectively in the system temporary directory.
