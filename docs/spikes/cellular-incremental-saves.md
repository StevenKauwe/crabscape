# Incremental cellular save prototype

The shipping browser save is a single v1/v2 record. It refuses snapshots above
its 12 MiB raw or 256 sleeping-chunk bounds and retains the previous save. The
prototype in `tools/cellstore-prototype` explores a different protocol; it is
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

## Prototype and remaining work

Run `python3 -m unittest discover -s tools/cellstore-prototype -v`. The small
serializable-transaction model tests staged publication, content reuse, bounded
batches, fail-closed conflicts, collection reachability and reset fencing across
writers. It deliberately assumes transactions provide the stated semantics: it
does not test IndexedDB event scheduling, storage eviction, quota, browser
crashes, or worker lifetimes.

`tools/cellstore-prototype/indexeddb.js` is a separate experimental implementation
of those transactions, using its own database and no game integration. Serve the
repository over localhost and open `tools/cellstore-prototype/test.html` in a
browser to exercise it with two connections. The page's `data-result` reports
PASS or the failure. This prototype still lacks production decoding, capture,
crash recovery, quota testing and a bounded, paginated collector. Neither
prototype establishes that enabling the format in the game is safe.

A production implementation still needs actual IndexedDB integration tests in
multiple tabs, reset/worker/transaction races, crash and quota injection,
version migration tests, integrity validation, stale-stage reclamation, and
WebKit/iPhone memory and performance measurements. Most importantly, loading
all chunks into the current in-memory `World` can exceed phone memory even when
individual chunks are small. Raising storage limits also requires bounded lazy
world reconstruction or explicit expanded-memory limits; chunked storage alone
must not relax the existing startup protections.

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

## Isolated end-to-end proof

`FrozenRestore` consumes bounded active and sleeping parts individually,
rejects missing, duplicate, malformed or oversized parts, and checks the
assembled world with the existing checkpoint validator. It eagerly holds all
accepted chunks and currently caps sleeping chunks at 256; it is not a lazy
loader. The isolated IndexedDB prototype can pin a manifest while reading it in
pages, so collection cannot delete an old snapshot between page transactions.
Reset invalidates the pin using the persistent epoch.

The standalone Wasm harness can be built and served from the repository root:

```sh
cargo build --release --target wasm32-unknown-unknown --manifest-path tools/cellstore-prototype/harness/Cargo.toml
wasm-bindgen --target web --out-dir tools/cellstore-prototype/pkg tools/cellstore-prototype/harness/target/wasm32-unknown-unknown/release/cellstore_proof.wasm
python3 -m http.server
```

Open `tools/cellstore-prototype/proof.html` on that server. It freezes a real
streamed terrain fixture, stages and publishes its parts, reloads the browser page, then restores four parts at
a time, compares the entire reconstructed terrain and fixture player/tick, and
checks interruption and reset behavior. A desktop Chrome run with 120 sleeping
chunks used 46 pages: the frozen snapshot's logical heap estimate was 10,995,023
bytes and the highest observed Wasm linear-memory **capacity** was 58,392,576
bytes. The latter includes the harness's original world, snapshot, restoration,
and comparison copies; it is not an allocation peak and excludes JS,
IndexedDB, browser overhead, and actual game assets. It does not establish a
mobile memory budget. The player is a small opaque fixture, not game progress.

This proof does not yet verify cryptographic manifest/part integrity, install
into the game, reconstruct on demand, migrate legacy saves, reclaim abandoned
pins after crashes, or test quota and WebKit. A corrupt manifest that is
internally plausible can still select the wrong chunks. Keep the live v1/v2
path and its existing limits until these problems are solved and measured.
