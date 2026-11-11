# Page-round latency model, 2026-11-11

Status: diagnostic only. No production batch-limit change is justified by this model.

## Reproduction

Baseline `cee179bb020cdd268c7303f76ef64366b5638609`, Rust 1.99.1, ARM64 Mac,
repository release profile:

```
RUSTC_WRAPPER= cargo run --offline --locked --release -p crabsim --example page_round_budget
```

The example starts the populated canyon at `(6250,-5000)`, with the same Hulk,
swimming99 and Feast fixtures as the headless live browser route. Controls follow
simulation ticks rather than wall time: 1,200 ticks east/up, 1,200 down, then
1,200 west/up. Each model frame attempts four120Hz ticks (30Hz display proxy).
A storage read returns after six model frames (200ms). Each run ends at exactly
3,600 ticks, not after a fixed wall-clock duration.

Terrain and memory records are stored in in-memory durable maps. Delayed save
snapshots retain their captured cuts until publication; publication waits for an
outstanding read to finish, and blocks a new read while due. Saved cuts are
acknowledged through the existing production APIs. This is a simplified scheduler,
not a full model of IndexedDB transactions, revision validation, active checkpoint
capture, compression workers, browser scheduling or rendering. It cannot establish
that a browser stall has a single cause.

Every accepted simulation tick is replayed in a separate fully resident simulation.
All final mover fields and active material/flow/grain/pending-fall checkpoint bytes
match. Runs with different caps also match these complete outputs at the same tick.
Resident sleeping pages remain bounded by256. The cap does not change that budget.

## Results

Caps12,24,32 respectively, with200ms read latency:

- Immediate save publication: total blocked frames573,517,529; longest55,37,37.
- One-second save publication delay: blocked584,556,555; longest54,36,36.
- Three-second save publication delay: blocked633,611,611; longest54,36,36.
- **Post-startup longest stall is11 frames (~367ms) in every case.** The larger cap
  reduces startup paging rounds but does not improve this route's worst later stall.
- With one-second save delay,32/33/33 batches have exactly one request while a flush
  is required, even when asked for256. At three seconds,62/65/65 do. This identifies
  cases where the batch cap is not limiting; it does not distinguish one urgent
  dependency from admission capacity on its own.
- Largest actual batch:479,277bytes at cap12, versus585,783bytes at cap24/32 with
  the three-second save delay. Peak resident sleeping pages:171 versus190.

Exploratory native reply-application maxima were about0.15–0.22ms. They include
record decoding/admission and exclude browser outer-record decompression, JS/Wasm
copies and IndexedDB callbacks; background compilation was not controlled. They
are not mobile timings or a calibrated budget.

Initial read-latency sweeps with immediate saves also showed cap32 was sometimes
worse in total blocked frames than24, despite the same final world. Paging and
save scheduling interact; more speculative work can change subsequent cache use.

## Decision

Do not raise the cap alone. `PageCache::requests` serves urgent demand first and
limits admissions to free or clean replaceable slots, excluding protected/dirty
pages. Increasing the requested cap cannot help a single-request dirty/flush wait.

The browser decoder also permits up to4MiB per terrain or memory record. A larger
count cap increases worst-case transient decoded memory even when this fixture's
responses stay under600KB. Any future larger-batch experiment needs an aggregate
byte budget or independently validated tighter per-kind limits and mobile testing.

The live hardware-GPU trace independently measures substantial fixed open/read/
final-validation waits. A controlled experiment with one success-only reusable
IndexedDB connection is a more promising next step than a cap-only patch. It must
retain epoch/revision/session validation, close on versionchange/reset, discard
failed/cancelled leases, and be tested through actual production paging, including
faults and recovery. This model does not authorize dropping any validation.

Logs for the final model are `/tmp/page-round-model-final.log`; the earlier
read-delay sweep is `/tmp/page-round-budget.log`. These are local diagnostics, not
committed generated artifacts. No real player save is read or modified.
