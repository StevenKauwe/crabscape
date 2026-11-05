# Solo paging stall investigation, 2026-11-05

Status: reproduced; no gameplay fix established. Do not close the transition-stutter report.

The existing game already publishes per-render solo simulation progress when
`data-cell-performance-probe` is set (`src/replay.rs`). The frame probe now enables
that interface rather than relying on the co-op-only tick counter. It reports
observed tick samples and longest unchanged-tick interval independently of position
and frame gaps. Missing samples break a run; absent telemetry reports null, not zero.
A run extending to the final frame is still a lower bound, not proof it recovered.
Passive IndexedDB open/get listeners retain request timings in the raw measures.
These listeners and CPU sampling add overhead and are diagnostic, not game code.

## Current production reproduction

Command (isolated profile, synthetic save, no player data):

```sh
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin frame_probe -- \
  --url https://crabscape.com/ --route canyon --large-save --slow 4 --cpu
```

Build `4900a63cb2c0e335021f1738efc5924bf5aac2ac`, M2 Max / ANGLE Metal,
4x CDP CPU throttling, 9,851,246-byte fixture with 924 spatial records:

- 1,315 observed frames/ticks; longest unchanged tick: 1,233.3 ms at tick 2449.
- The pause recovered; it spans approximately 49,621–50,855 ms on the trace clock.
- Frame median/p95/max: 50.0/66.7/100.1 ms. Rendering continued during the tick pause.
- During that pause, 16 non-metadata IndexedDB gets referenced 13 distinct keys;
  three terrain records were read twice. Correlation does not establish the complete cause.
- Trace: system temporary directory `crab-frame-probe-94596`; log:
  `/tmp/crab528-live-solo-storage.log`.

A separate live co-op handoff regression passed traversal and native replay checks:
no frame gaps above 60 ms, no host tick stalls above 75 ms, one guest tick stall
of 83.3 ms. That route did not reproduce the severe solo behavior.
Log: `/tmp/crab528-handoff-current.log`; host/guest traces `crabscape-86335-*`.

## Experiments preserved, not shipped

A local diagnostic build based on the same main reproduced 1,649.9 ms and
1,983.3 ms tick pauses (`crab-frame-probe-88373`, `-89266`). A candidate combined
the final reset-epoch and head-revision validation into one read-only transaction,
retaining both guards. Its capture ended with an ongoing 3,599.9 ms tick pause:
88 non-metadata gets over 22 keys, several records requested six or seven times
(`crab-frame-probe-92341`). Different routes and workloads prohibit causal timing
comparison, but this does not demonstrate a fix. Production edits were restored;
the experimental patch remains `/tmp/crab-solo-validation-experiment.patch`.

A native exploratory crossing from (6250,-5000) to (6800,-4300) requested 30
pages and needed three reply batches with a limit of 12, versus two with 32.
No production batch limit was changed. The probe is preserved at
`/tmp/crab-transition-batch-probe.rs`; output `/tmp/crab-transition-batch-probe.log`.

Next: reproduce repeated reads with deterministic cache/storage scheduling,
separate legitimate revision retries from cache churn, and retain bounded memory,
reset-epoch/revision validation, terrain equivalence and save integrity. Current
observations do not justify weakening those checks or claiming all stutter fixed.
