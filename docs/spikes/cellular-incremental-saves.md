# Incremental cellular save prototype

The shipping browser save is a single v1/v2 record. It refuses snapshots above
its 12 MiB raw or 256 sleeping-chunk bounds and retains the previous save. The
model in `tools/cellstore-proof` explores a different protocol; it is
not wired into the game or IndexedDB and does not raise those limits.

## Required consistency

A save must describe one simulation tick: active cells and water, all sleeping
chunks, pebble ownership and global metadata, player position and progress, and
the tick must agree. Sleeping chunks can be shared between successive saves
only when their exact bytes and interpretation are unchanged. Global pebble IDs
and the active window make this more than saving independent images of terrain.

Incremental IO alone cannot produce that snapshot. Reading live chunks across
frames can combine two states, or omit a chunk that moved between active and
sleeping. Before using the protocol, implement a bounded frozen snapshot, such
as versioned immutable chunks with copy-on-write, and capture the player,
active state, tick, and chunk-version list together. Measure its peak heap and
frame cost on mobile. Dirty tracking must include every mutation, including
streaming, flow, grain, owner, and removal; a missing dirty flag silently loses
world state.

## Proposed storage transaction boundaries

Use one per-build IndexedDB database and a common object-store transaction scope
for the persistent epoch, published manifest, immutable chunks, and staging
records. Every transaction below must abort completely on request, quota, or
commit failure. These steps describe required semantics, not existing code:

1. In one transaction reserve a unique staging ID, capture the persistent epoch
   and current manifest revision, and enforce bounded concurrent writers. Do not
   hold an IndexedDB transaction open across awaits or worker messages.
2. Stage bounded batches of new immutable chunk records. Each batch transaction
   rechecks the persistent epoch and stage ownership, writes chunk bytes and
   appends their identifiers to the stage atomically. Unchanged chunks may be
   referenced by a new stage only after verifying the immutable key and content;
   do not treat a short checksum as a collision-proof identity. Use per-record
   length and integrity checks and a collision-resistant digest or unique IDs.
3. After all chunks and the consistent tick/player/active state are ready,
   atomically validate the epoch, expected base revision and complete ordered
   manifest, then publish the new manifest and retire its staging record in the
   same transaction. A competing writer or reset causes a conflict, leaving
   the prior published manifest intact. Report quota and storage errors; never
   convert a failed load into a new save automatically.
4. Garbage collection computes reachability from the published manifest **and
   every live stage**, then deletes unreferenced records within the same
   transaction scope. It must not delete chunks referenced by a concurrent
   reader or stage. Reclaiming crash-orphaned stages needs a cross-tab ownership
   protocol with fencing; elapsed wall-clock time alone is unsafe. Until that
   protocol is implemented, retaining orphans is safer than deleting live data.
5. Reset atomically increments a persistent epoch, clears the manifest, chunks,
   stages and legacy cellular record, and retains the new epoch. Every later
   stage or publish rechecks it in its own transaction, so work from another
   tab or a pre-reset worker cannot resurrect the old save. Pause the current
   game through success or error, as the existing browser reset does.

Readers need a consistent manifest plus all its chunks: use one IndexedDB
readonly transaction or an explicit pin/lease protocol. Merely reading the
manifest and opening a later transaction allows collection to remove its chunks.
Every record and declared collection length must be checked before allocating;
validate full world and player state before installation. Do not fall back to a
legacy save if a newer published manifest exists but is unreadable, since this
would hide loss and permit overwriting it. Migration should publish the new
format before considering removal of the old v1/v2 record; until then preserve
it byte-for-byte. Existing normal-game saves remain separate.

## Prototypes and remaining work

The standalone Rust model in `tools/cellstore-proof` tests staged publication,
SHA-256 part and manifest integrity, bounded verification and garbage-collection
steps, competing writers, explicit recovery, and epoch fencing. Run
`cargo test --manifest-path tools/cellstore-proof/Cargo.toml`. Its operations
are synchronous in-memory transactions, not IndexedDB; real cross-tab ordering,
crashes and quota must be verified separately. Hashes detect accidental damage;
they cannot authenticate against an attacker who can replace the data and its
digest. Recovery deliberately fences even a slow writer and never reuses its
ID; automatic detection of abandoned owners remains unsolved.

The unwired `underwater::versioned_chunks` experiment demonstrates the smaller
copy-on-write mechanism: a snapshot shares immutable payloads and copies an
ordered index; a later mutation clones only the affected payload. Its bounded
pages borrow data from that frozen snapshot. The model requires payloads and
metadata without interior mutability, does not bound key heap size, and is not
connected to real streamed terrain. A production design must capture the active
window, player and global pebble table atomically with the chunk index, ensure
every mutation uses copy-on-write, and deal with snapshots held while many
chunks change. Lazy disk reconstruction additionally needs per-chunk validation,
an IO cache budget, deterministic missing-chunk behavior, cancellation and reset
fencing, and a policy for when simulation may advance while a chunk is loading.

`FrozenRestore` consumes bounded active and sleeping parts individually,
rejects missing, duplicate, malformed or oversized parts, and checks the
assembled world with the existing checkpoint validator. It eagerly holds all
accepted chunks and currently caps sleeping chunks at 256; it is not a lazy
loader. Its Rust tests exercise exact reconstruction and malformed inputs.

An earlier throwaway browser experiment (not part of the repository or game
pipeline) staged and verified SHA-256-addressed parts in IndexedDB, reloaded the
page, and reconstructed a streamed terrain fixture four parts at a time. Chrome
passed with 120 sleeping chunks over 46 pages, including corruption, missing
parts, interrupted staging and reset checks. The frozen snapshot's logical heap
estimate was 10,995,023 bytes; the highest observed Wasm linear-memory capacity
was 58,392,576 bytes. The latter includes original, snapshot, restoration and
comparison copies; it is not an allocation peak and excludes JS, IndexedDB,
browser overhead and game assets. The player was an opaque fixture, not game
progress. These results do not establish a mobile memory budget.

Production still requires a Rust/Wasm IndexedDB implementation, automatic
crash and stale-pin recovery, safe bounded collection, quota handling, real
player/progress capture, lazy world loading, legacy migration, and WebKit/iPhone
validation. Do not relax existing live v1/v2 protections based on these models.

## Rust/Wasm IndexedDB primitive

The standalone Rust proof crate now includes an unwired IndexedDB primitive
under `indexeddb`, gated to Wasm and restricted to names beginning with
`crabscape.chunk-proof.`. It uses a separate database, a single writer, bounded
binary metadata (512 parts, 4 KiB player data), batches of at most four parts
and 128 KiB per part. A single read/write transaction commits each batch and
its stage together; publication swaps the head only after the expected count
is present. A persistent epoch and monotonic stage ID fence reset and explicit
abandon across connections. Individual part reads check SHA-256 and reject a
stale head. SHA-256 also protects metadata and the full manifest against
accidental corruption (not against an attacker able to rewrite the digest).
One uniquely fenced pin retains an older head while a newer one is published.
Explicit recovery can discover and fence a pending writer or pin. Collection
scans at most 32 keys per page and rechecks the persistent epoch and all live
head, stage and pin references in its deletion transaction. Unknown keys are
retained. Abandon leaves orphaned part records for later collection.

To run the isolated Chrome proof, install the `wasm32-unknown-unknown` target,
`wasm-bindgen` CLI and Chrome, then run:

```sh
cargo run --manifest-path tools/cellstore-proof/Cargo.toml --features browser-runner --bin browser-proof
```

Set `CHROME_BIN` when Chrome is not at the default path. The Rust runner builds
the Wasm proof, serves it on loopback and uses a temporary browser profile;
its minimal generated browser bootstrap invokes the Rust test. The test covers
publication abort, previous-head preservation, crash-stage discovery, explicit
abandonment, pinned readers, paged collection, malformed records, part
integrity and reset against a second database connection. Native record
and model tests also run with `cargo test --manifest-path
tools/cellstore-proof/Cargo.toml`.

This remains a transaction experiment. It does not automatically determine
whether a writer or pin belongs to a crashed tab, automatically schedule or
resume collection, bound JavaScript's structured-clone memory, handle quota
recovery, migrate the old record, integrate the game, or establish mobile/WebKit
safety. The experimental metadata encoding changed during development and is
not a stable format. A forced transaction abort is tested; actual quota
exhaustion is not. Reset deliberately clears only the separate experimental
database. Do not substitute it for the live save path.

## Lazy terrain view experiment

The unwired `LazyView` consumes the frozen metadata plus an expected sleeping
page index, keeps at most eight pages, and reports missing pages instead of
inventing terrain. It preserves one-based pebble owner IDs and the frozen
global pebble table. An aggregate cannot be exposed as complete until every
page intersecting its footprint is resident and agrees with that owner. Load
tickets are tied to one view instance and invalidation rejects late results;
the caller must also enforce the persistent storage epoch. Page hashes are
verified by the storage layer before the view receives bytes.

Rust tests exercise active-page and active/sleeping boundaries, exact sleeping
cells, eviction/reload, invalid owners, incomplete reads and invalidation. The
isolated Chrome proof reads 180 real active/sleeping pages through pinned
IndexedDB, compares active material and flow against the fixture, and rejects a
late page after reset. It observed two resident pages, a 304,383-byte logical
view estimate (including room for the incoming page), and 26,083,328 bytes of
Wasm linear-memory capacity for the whole test module, including its original
world and full frozen snapshot. This is not a measured total heap peak: it
excludes browser/JS storage, allocator overhead, caller-owned buffers, and game
assets, and has not been measured on mobile. The proof supplies the sleeping
index from its in-memory capture; it does not yet reconstruct that index from a
persisted manifest or install and advance a playable world. Its page limit,
validation rules, memory estimate and experimental format are not production
compatibility guarantees.
