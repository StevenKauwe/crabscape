# Solo paging stall investigation, 2026-11-05

Status: reproduced; publication/read coordination mitigates one cause. Additional pauses remain; do not close the transition-stutter report.

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

## Publication/read coordination

Profiling-only page-state and reply-outcome markers attributed ten discarded
read batches in `crab-frame-probe-97946` to autosave revision publication while
reads were in flight. Several 650–783 ms pauses include a discarded reply followed
by another read of the required pages. There were no read-failure markers.

The coordination gate leaves encoding/compression concurrent with paging. At final
publication only, it blocks new reads and waits for existing replies to be applied
(or discarded on failure/cancellation). It holds publication until the transaction
returns. The caller then updates its revision without yielding. No epoch, revision,
record validation, transaction atomicity or failed-write protection is relaxed.
Read permits travel with the asynchronous result, so I/O completion alone cannot
allow publication before the simulation has consumed the reply. RAII releases
permits on failure/reset/cancellation. The existing read deadline remains in force.

Five dependency-free gate tests cover ordering, cancellation, multiple outstanding
replies, wakeup and duplicate publishers. The native delayed-storage model uses
200 ms reply latency, completes 3,361 ticks, and matches an unpaged simulation's
mover and active terrain byte-for-byte. This model is an opt-in investigation test,
not proof that browser scheduling has the same latency.

The same synthetic browser route with the gate (`crab-frame-probe-1305`) recorded
159 applied replies and zero stale or failed replies. The baseline had 169 applied
and ten stale replies. Routes differ with wall-clock input scheduling, so frame
and maximum-pause comparisons are not controlled. In particular, the candidate
still recorded a 2,116.5 ms tick pause while saving and requesting a flush, with
repeated applied reads. This change eliminates the demonstrated publication race;
it does not establish a universal speedup or resolve the remaining save/cache wait.

Evidence: `/tmp/crab-page-state-capture.log`, `/tmp/crab-page-access-capture.log`,
`/tmp/crab-native-solo-paging.log`. Reproduction command is unchanged above.

## Follow-up local phase instrumentation (not part of #529)

Trace `crab-frame-probe-8090`, log `/tmp/crab-save-phase-capture.log`, adds
compression roundtrip, record preparation, publication-wait/transaction and
individual terrain admission markers. Same synthetic fixture, 4x CPU throttle
and CPU sampling; route timing is not controlled against earlier captures.

- 698 terrain pages installed, no deferred terrain admissions.
- 1,073 sequential compression roundtrips total 49,931 ms (mean 46.5 ms).
- Nine completed save preparations average 6,113 ms (maximum 9,929 ms).
- Publication waits average 128 ms, transactions 237 ms; full writes average 6,478 ms.
- Longest unchanged tick is 799.9 ms at tick 1721 (42,221–43,021 ms).
  It resumes before that save finishes at approximately 44,014 ms. Thus expensive
  save preparation alone is not proven to cause this simulation pause.
- Delaying the native model's save flush by two or six seconds still produces
  no pause above the initial 18-frame/600-ms load. Both runs match resident
  movement and terrain. This simplified model does not capture browser scheduling.

The candidate batches up to 16 records and 4 MiB of raw data per worker message.
Each record retains its existing compression, verification and stored format.
Raw staging and encoded transaction bytes are bounded; cancellation, revision
validation and atomic publication are retained.

The delayed-save probe remains local in the original solo-transition-trace
worktree. Its logs are /tmp/crab-delayed-save-60.log and
/tmp/crab-delayed-save-180.log. The #529 publication fix landed at b4efb31.

## Bounded batch candidate

Canyon capture `crab-frame-probe-14770` uses the same synthetic fixture and 4x
CPU throttle as the phase-instrumented baseline. Its 23 completed writes average
992 ms record preparation and 1,415 ms total write time, versus baseline 6,113 ms
and 6,478 ms. Worker roundtrips decrease from 1,073 single-record jobs to 135
bounded batch jobs, despite more completed saves. All 701 terrain replies were
installed. The browser save suite passes failed-publication recovery, fresh and
retired-format reloads, retention of 390 sleeping chunks, and a 400-page owner's
co-op join/leave/save with an exact shared simulation hash.

This is a save-throughput improvement, not a proven general frame-pacing fix.
The candidate traverses a different wall-clock-driven route and still pauses
simulation for 1,217 ms; frame p95/max are 66.7/100 ms versus baseline 66.5/83.4 ms.
Do not infer a gameplay FPS gain or attribute the difference to batching alone.

A controlled worker-only comparison runs identical 128 x 32-KiB records in
serial/batch/batch/serial order while the opening game renders, on the same M2 Max,
4x throttle, with no concurrent builds. Every compressed byte matches across modes:

| Mode | Time | Observed frames | Max frame | Frames >60 ms |
|---|---:|---:|---:|---:|
| Serial | 6,080 ms | 127 | 83.4 ms | 14 |
| Batch | 330 ms | 7 | 50.0 ms | 0 |
| Batch | 323 ms | 7 | 50.0 ms | 0 |
| Serial | 5,869 ms | 127 | 83.3 ms | 7 |

The batches' seven-frame samples are too short for a general responsiveness claim.
This isolates worker message overhead with synthetic compressible records; it does
not include save capture, main-thread record serialization or database commits.
The real canyon capture above covers those parts, with the route limitation noted.

Run against a locally built/served candidate in a disposable browser profile:

```sh
CRABSCAPE_PROFILE_GPU=1 cargo run -p cooptest --bin worker_batch_probe -- http://127.0.0.1:PORT/
```

Evidence: `/tmp/crab-batch-capture.log`, `/tmp/crab-batch-save-test.log`,
`/tmp/crab-worker-batch-probe.log`. No hard timing gate is implied by this probe.
