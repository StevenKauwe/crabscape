# Validated paging connection reuse

The delayed-storage model in `page-round-model.md` rejects a batch-cap-only fix:
its worst post-startup pause remains unchanged across caps 12, 24 and 32. Live
hardware-rendered tracing separately finds substantial repeated connection-open,
read-transaction and final-validation waits. The candidate removes only the
connection-open wait on a successful cache hit.

## Ownership and failure rules

- At most one idle `IdbDatabase` lease, keyed by database name and reset epoch.
- A lease is removed from the idle slot while a read owns it. Only an entirely
  successful read, including unchanged final epoch/revision validation, returns it.
- Failed/cancelled reads drop the lease and close its database. The existing
  abort guard, five-second deadline, late-open delivery and single-open gate stay.
- A `versionchange` handler marks the lease closed and closes its database.
  Closed leases are never reused or retained.
- Startup configuration and both reset APIs clear idle state and advance a
  runtime-only generation. Old in-flight reads and late opens cannot refill it.
- Record formats, save publication atomicity, page count/resident limits, retry
  interval, epoch/revision/session validation and simulation state are unchanged.

## Functional proof

After rebase onto `ff427e0`, the served game artifact is
`b02a66e6f0314cc377a0435945d951b18695db19`. Release Wasm build and Wasm Clippy
(`-D warnings`) pass. Both headless probe binaries pass Clippy.

The actual root-game command `cargo test --offline --locked -p crabscape --release
cell_save::` passes all 47 selected tests, including all four slot lifecycle,
five read-deadline, five publication-gate and two read-priority tests, as well as
save compatibility, reset and replay coverage. Native all-target Clippy passes.
The earlier standalone helper tests also passed; they are superseded by this
real-crate test run. This is filtered coverage, not the entire workspace suite.
After the later rebase onto `e0d4090` (#557), all 47 filtered tests pass again,
as do native all-target and Wasm Clippy. Cloud CI remains required.

The actual production `page_validation_probe` passes all eight injected cases in
a disposable browser profile with synthetic saves and no clipboard permission:

1. Changed reset epoch, changed revision, changed head epoch, missing head and
   damaged head are each rejected before application, then recover normally.
2. Externally closing a cached connection causes one failed read, disposal, then
   successful application through a different connection object.
3. A real IndexedDB version-upgrade request invokes `versionchange`. Its upgrade
   transaction is deliberately aborted so the database and saved records remain
   unchanged. The cached connection closes and subsequent paging opens another.
4. Suppressing the batch's completion events invokes the production five-second
   timeout. The connection closes and a new connection successfully retries.

The probes exercise browser callbacks and production paging, not a replacement
JavaScript paging algorithm. Initial and post-rebase runs both pass. Log:
`/tmp/boundary-production-faults-final.log` (local diagnostic, not committed).

The production probe's `--reuse-only` mode separately observes actual readonly
terrain/memory batch transactions and their `IdbDatabase` object identities. On
this artifact, eight completed and applied batches use exactly one connection,
with zero aborted transactions or failed replies. The same eight-batch probe on
`https://crabscape.com/`, served build
`3a453d2e5274a6896cf9fd6a27afee1bb7170135`, observes eight separate connection
objects, also with zero aborts/failures. Candidate build is the `b02a66e` artifact
above. This confirms an eight-to-one connection reduction without
relying on wall-clock timing or counting unrelated startup/autosave connections.

The separate `connection_probe` measures a fixed synthetic IndexedDB workload;
it explicitly does not implement production epoch/revision validation. It can
isolate repeated-open cost but cannot establish phone FPS or terrain correctness.
Timing was initially deferred because whole-machine compiler contention persisted
despite pausing our workers. The later controlled result below supersedes that
blocker for this specific initial-paging workload; no traversal or FPS improvement
is claimed.

## Controlled production paging follow-up

A later quiet A/B/B/A sequence compares CI-built baseline `ea7239d9` (includes
#557; its additional #562 change is CI-only) with candidate `e2c82315` (the CI
merge of PR head `e0f6a89` into `e0d4090`). Both artifacts use the same local Rust
HTTP server, separate disposable browser profiles, the same synthetic canyon
save, hardware rendering and 4x CDP CPU throttling. This is a desktop proxy,
not physical-phone testing.

The probe selects eight **complete** production read spans after attachment;
it excludes the initial read already in progress. All four runs have identical
ordered 127 page keys, eight completed/applied batches and zero failed/aborted
reads. Each of the five timing series contains exactly eight measurements.
The baseline uses eight database connections; the warmed candidate uses one.

| Run | Sum of read durations (ms) | Eight-read elapsed (ms) | Sum of open waits (ms) | Maximum read (ms) |
| --- | ---: | ---: | ---: | ---: |
| A1 baseline | 1076.3 | 1290.3 | 227.3 | 158.0 |
| B1 candidate | 719.8 | 942.8 | 0.1 | 113.8 |
| B2 candidate | 797.4 | 1023.9 | 0.0 | 133.5 |
| A2 baseline | 1209.3 | 1450.5 | 280.3 | 200.0 |

Pooled eight-read elapsed is about 28.2% lower and the sum of read durations is
about 33.6% lower. These are two invocations per version, initial paging after
warming the connection, not a full traversal or FPS result. Final-validation
waits remain substantial. The patch is not established to eliminate all boundary
stalls or dirty-cache publication waits.

Whole-machine checks show 96% idle before and 95.46% after. One-second process
snapshots contain no competing compiler, test or extraction; workers explicitly
paused, transfers had finished, and no thermal warnings were reported. Raw files
are `boundary-production-clean-{a1,b1,b2,a2}.json` and
`boundary-production-abba-clean-os-load.txt` in the local temporary directory.

An earlier attempted sequence is excluded in full: an unrelated artifact
transfer/extraction overlapped it, and the utility's initial eight-application
selection included a partial startup read. Its assertion threw, but the shared
CDP evaluator exposed an empty object without failing the executable. No timing
from that attempt is used. The utility now reports exceptions explicitly, rejects
missing/partial result shapes, tests that empty/null/exception objects fail, and
selects eight complete read spans. Baseline and candidate functional checks also
verified identical keys before the clean sequence.

A separate synthetic IndexedDB-only A/B/B/A microbenchmark was inconclusive:
64-batch totals were 240.9/166.9/210.4/170.1 ms and maximum batch times stayed
55–57 ms. That workload does not include production paging validation and is not
used to claim a speedup.

Final utility verification: all six utility/fixture tests pass, Clippy passes,
and all eight production fault cases pass again on CI artifact `e2c82315` with
explicit exception-envelope checks. These checks are local to this probe;
the shared CDP helper is unchanged.
