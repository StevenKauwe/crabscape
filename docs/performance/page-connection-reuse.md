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

Four standalone `rustc --test` tests of the exact generic slot source pass:
matching name/epoch reuse, immediate replacement disposal, reset/old-generation
rejection, and unretained cancellation disposal. Five standalone tests of the
unchanged deadline source pass, including timeout/late-open resource disposal.
These are not a whole-game `cargo test` run; required CI remains necessary.

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
with zero aborted transactions or failed replies. This confirms reuse without
relying on wall-clock timing or counting unrelated startup/autosave connections.

The separate `connection_probe` measures a fixed synthetic IndexedDB workload;
it explicitly does not implement production epoch/revision validation. It can
isolate repeated-open cost but cannot establish phone FPS or terrain correctness.
No performance result is claimed until a coordinated quiet measurement completes.
