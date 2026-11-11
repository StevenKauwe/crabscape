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
After the later rebase onto `e0d4090` (#557), the filtered run is being repeated;
cloud CI remains required.

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
No performance result is claimed until a coordinated quiet measurement completes.
