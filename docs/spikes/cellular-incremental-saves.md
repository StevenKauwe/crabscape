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

The internal `FrozenRestore` consumes bounded active and sleeping parts individually,
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

Production still requires a fully integrated and independently validated
Rust/Wasm IndexedDB implementation, automatic crash and stale-pin recovery,
quota handling, real
player/progress capture, lazy world loading, legacy migration, and WebKit/iPhone
validation. Do not relax existing live v1/v2 protections based on these models.

## Rust/Wasm IndexedDB primitive

The standalone Rust proof crate now includes an unwired IndexedDB primitive
under `indexeddb`, gated to Wasm and restricted to names beginning with
`crabscape.chunk-proof.`. It uses a separate database, a single writer, bounded
binary metadata (512 parts, 64 KiB player data), batches of at most four parts
and 128 KiB per part. A single read/write transaction commits each batch and
its stage together; publication verifies all staged parts sequentially in the
same transaction before swapping the head. Checksummed metadata records which
head, stage and pin must exist, so a missing ownership record fails closed.
Each session must retain its captured epoch when beginning a save; a persistent
epoch and monotonic stage ID fence reset and explicit abandon across
connections. Individual part reads check SHA-256 and reject a
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
not a stable format. A forced transaction abort and a real asynchronous
IndexedDB duplicate-key error are tested; actual quota exhaustion is not.
Reset deliberately clears only the separate experimental
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
cells, eviction/reload, invalid owners, omitted pebble-owner cells, incomplete
reads and invalidation. A bounded inverse-footprint check rejects a page where
the global pebble table expects ownership but a cell has none. The
isolated Chrome proof reads 180 real active/sleeping pages through pinned
IndexedDB, compares active material and flow against the fixture, and rejects a
late page after reset. It observed two resident pages, a 304,383-byte logical
view estimate (including room for the incoming page), and 26,083,328 bytes of
Wasm linear-memory capacity for the whole test module, including its original
world and full frozen snapshot. This is not a measured total heap peak: it
excludes browser/JS storage, allocator overhead, caller-owned buffers, and game
assets, and has not been measured on mobile. Its page limit, validation rules,
memory estimate and experimental format are not production compatibility
guarantees.

`CaptureIndex` now derives a canonical ordered page index and digests from the
frozen capture. In the isolated browser proof it is stored with the snapshot,
then read from the same pinned head as the metadata and every page. An
`IndexVerifier` checks tick, count, page ordering and each digest, streams all
pages through the two-page validator, and checks that global pebble footprints
have declared pages. Its hash callback must be collision-resistant; the proof
uses SHA-256. This confirms consistency with the supplied index, not that an
arbitrary self-consistent index is complete. Completeness still depends on
trusted capture provenance. The index also binds the saved player bytes and tick. Reset
and pin validity must be checked before using the result. Index construction
currently encodes and hashes all pages synchronously and is not a production
mobile performance design.

## Guarded active-world experiment

`GuardedBuilder` can install the active window only after every active part
validates. It requires a **complete, trusted** sleeping-page index and a
trusted game configuration. The isolated `from_saved` path reconstructs a player
from at most 64 KiB of version 2 save JSON, bound to the verified index and tick,
instead of requiring an in-memory `Sim` template. Each subsequently fetched
active or sleeping page is checked again against its verified index digest. The
input byte limit is not a
strict heap limit; configuration and capture provenance must be trusted. Actors
are not restored by this proof, matching the existing save format limitation. An incomplete index can make a missing region look pristine, so this
prototype must not be used with an unverified or partial manifest. Recording
is rejected while terrain is incomplete.

`GuardedPlayable` advances a tick only when any potential active-window shift
has all saved sleeping pages overlapping both old and new windows. Otherwise
it returns the required page IDs without changing simulation state. Accepted
pages retain their original global pebble IDs. Reset invalidates the session
and rejects late page responses. The prototype limits loaded sleeping chunks
to 256 and clones terrain to preflight near a window edge; it is not a memory
budget or a deployable streaming engine. Its edge trigger currently mirrors
`follow_player` rather than sharing the same implementation, so a future
change there could invalidate the guard. Hydrating all pages permits the
caller to retrieve the complete `Sim`; replay capture is tested only after full
hydration at a reproducible startup, not during partially loaded play.

Native tests compare each guarded step to a full reference, exercise a blocked
transition and nondefault player progress, and replay the fully hydrated result
to the exact reference hash. A Rust-hosted Chrome proof installs active pages
from the isolated IndexedDB store, checks player steps, hydrates the saved
sleeping pages, compares the resulting full state, and rejects a delayed page
after reset. Avoiding extra terrain clones and streaming the existing FNV hash
representation reduced the test's observed Wasm linear-memory capacity from
185,729,024 to 40,304,640 bytes on desktop Chrome. This is not total process
peak memory and excludes browser/JS storage; the test still holds a full source
world and frozen snapshot. It does not establish iPhone or WebKit safety.
Migration, trusted complete capture provenance, actor and replay parity, bounded
memory, mobile testing, and end-to-end storage integration remain required before
any live integration. A native regression reconstructs actual game save data with
nondefault progress and memory, compares player and tick through subsequent steps,
and compares terrain after full hydration. The browser fixture uses a minimal
version 2 player record and has not established mobile memory safety.

The Rust-hosted Chrome test now also writes the game's actual `SaveData` with
nondefault progress and memory, reloads the document and Wasm instance, then
reconstructs from IndexedDB. The first post-reload tick waits for required
sleeping pages; it walks 260 normal-tuning ticks through a second window
transition that requires another page wait. Player state and tick are checked
at each step; after full hydration, terrain and pebble ownership match the
reference. Reset rejects a held pending page. Its fixture contained 31 active
pebbles and 180 pages. Desktop Chrome reported 31,195,136 bytes of Wasm
linear-memory capacity on this run; capacity is not process peak memory and
includes a full reference and frozen fixture. This is not a mobile memory bound,
a quota/migration test, or live-save integration.

The game-binary native test also decodes genuine v1 and v2 envelopes, captures
each restored terrain and actual player state into the chunk format, verifies
every page and fully reconstructs the original terrain, while leaving the source
bytes unchanged and rejecting damaged input. This checks codec-to-capture
compatibility only: it does not publish a migrated save to IndexedDB, exercise
crash-safe migration or delete or replace the old save. Those remain open.

A later isolated Chrome proof builds test v1 and v2 wire envelopes, stores each
in a separate experimental IndexedDB source, and decodes it with the same
shared Rust decoder used by the game. It captures the restored player and terrain,
stages chunked data into a separate experimental destination, and rejects an
incomplete publish. After reopening that destination it fences the interrupted
stage while preserving its previous head and the source; a reset also fences a
stale migration writer. A fresh document then reloads each published destination,
checks that each source remains byte-identical, and runs the two-window playable
comparison. These are test-created envelopes and proof databases; this is not
yet migration from the live IndexedDB schema or a tested quota-exhaustion path.
The observed Wasm linear-memory capacity reached 65,077,248 bytes in the desktop
Chrome migration fixture; it is not a total process peak or an iPhone bound.

The migration fixture now applies the live player compatibility validator before
starting a destination stage. Checksummed v1 and v2 sources with impossible
upgrade counts or nonfinite XP are rejected while both the source bytes and the
previous destination head remain intact. Short old progress lists and upgrades
carried from another world remain acceptable. Before staging, the fixture also
verifies every candidate page, reconstructs the full terrain, and applies the
guarded player's terrain/position checks. A checksummed source with an unsafe
position is rejected with both source and prior target untouched. This
conservative validation can reject otherwise valid saves outside its active
window or above its 256 sleeping-page hydration limit; it retains multiple world
copies and is not a bounded-memory production migration.

The isolated migrator now holds a `PausedChunks` immutable borrow of the decoded
world while building the index and staging pages. This prevents mutation of that
world during the async operation without making a second frozen terrain copy.
Native tests compare every encoded active/sleeping page and the index with the
owned snapshot, and a compile-fail test checks the borrow cannot outlive a
mutation. In one native allocator probe of 120 sleeping chunks, borrowed view plus
index added a 126,260-byte peak, versus 11,007,339 bytes for owned snapshot plus
index. These figures exclude the original world, decoding, validation/reconstruction,
allocator overhead outside the probe, JS/IndexedDB and the game; they are not a
whole-process or mobile bound. Pausing is necessary for this view, and normal
live autosaving remains on the existing path. The migration proof still holds
several full world copies for decoding and candidate validation.

The separate proof databases do not give source reset and destination publish a
shared atomic fence. A source reset between reading and destination publication
can therefore leave a stale migrated destination; checking the source epoch just
before publication still leaves a race. Live migration needs one authoritative
reset epoch that both operations can enforce atomically, or an independently
proved cross-database coordination protocol. The current experiment must not be
used to claim cross-database reset safety.
