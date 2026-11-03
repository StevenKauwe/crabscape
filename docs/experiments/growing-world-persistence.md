# Saving a growing world

The spatial saver separates bounded runtime caches from durable coordinate records.
Periodic saves capture the active kernel and changed sleeping terrain pages without
cloning the explored terrain index. Personal memory uses sparse 32-by-32 pages with
visit counts and absolute last-seen steps; advancing its clock changes metadata only.
Unseen extent allocates no dense visits, age, visibility or obstruction grid. The
renderer uses a fixed 256-by-256 fog texture over the visible camera area.

Terrain and memory each retain at most 256 clean/dirty cache coordinates. Dirty
pressure requests a save; a region/window waits for missing pages and save capacity
before changing authoritative state. Storage requests are bounded batches of four
terrain and four memory pages. Terrain includes cell material, flow, suspended mass,
grains and pebble owners; passive falling work is bounded to 16 archive coordinates
per batch on native and Wasm. Pending falls remain mass-carrying saved state.

Native storage writes independent coordinate/version records and publishes a head
under 128 bytes after fsync. A transaction intent removes unpublished records before
retry. An OS file lock serializes writers across processes. Reset publishes a durable
new epoch before reclaiming old epoch directories; expected heads reject old writers.
After a successful publication, old versions of changed records are reclaimed.

Browser storage compresses bounded changed records off-thread, then replaces their
coordinate values and the small head in one IndexedDB transaction. There are no
staged payload blobs or retained application generations. IndexedDB rolls back failed
transactions and holds prior values for live database transactions. All multi-request
read candidates validate their epoch and head again before installation: publishing a
new head revokes prior read revisions. Both backends acknowledge immutable page cuts
only after publication, so a newer edit cannot be cleared by an older acknowledgement.

Portable character export explicitly assembles cold personal-memory records and
resident overrides before encoding; periodic saving never calls it. The existing
portable character size limit reports failure rather than truncating. Large solo
replay starts retain the existing explicit unavailable state; a solo recording is
also invalidated when a real terrain payload is evicted, since a partial cache is
not a reproducible full-world start. Regional replay kernels remain complete and
are checked against native playback.

These bounds cover terrain/map exploration. Existing level actors, shared resource
flags and their regional random streams keep their existing finite-level model;
this change does not introduce an infinite NPC spawning/world-generation system.
Pebble procedural sources remain the fixed shelf and only actual source chunks
allocate seed metadata. Arbitrary future entity generation requires its own spatial
record contract rather than extending the global level tables.

Native measurements at 100/1,000/10,000 personal-memory pages show 100/256/256
resident pages (8 KiB each), with one changed page captured. Terrain archives at 100/1,000/10,000 pages reach
161/161/162 resident pages (64 KiB each), with at most 120 changed pages per
window capture; cold revisits compare all active material, flow, grains and owner
bytes against a fully resident reference. The manual terrain scaling test repeats
this at 100, 1,000 and 10,000 archived pages.

The desktop failure on November 2 was specifically the independent 64 KiB player
record limit: a real character contained 264,201 bytes, including 263,068 characters
of remembered map data. A small terrain world reproduced the same rejection. The
immediate fix gives player metadata the existing metadata-page budget and tests a
fully remembered demo at the largest representable memory age. That prevents this
failure in the current demo; increasing a budget is not the huge-world architecture.

## Target storage contract

- Store terrain, personal map memory, entities and player metadata in independent,
  spatially keyed records. Keep the world head small; do not put every chunk ID or
  payload into one manifest or IndexedDB object.
- Keep bounded page sizes and a bounded working cache. Total explored-world size
  must not be a capture-time limit. Load cold chunks before entering their area and
  evict clean cached chunks; retain their durable records.
- Track changed chunk revisions. Freeze changed buffers with copy-on-write, encode
  them away from the frame loop, and commit only those records plus the world head.
  A failed transaction keeps the last complete head and its referenced records.
- Bind reads and writes to the reset epoch and committed snapshot revision. A late
  worker result, expired authority or interrupted load must not publish partial or
  mismatched state. Reclaim obsolete versions only after readers no longer need them.
- Browser storage can genuinely fill or be denied. Distinguish that from an internal
  record-bound violation; retain the last save and support portable export. Never
  discard exploration or silently truncate state to satisfy a budget.

## Map memory

Use sparse coordinate chunks, allocated only when seen, rather than arrays over the
whole level. A memory cell stores visit count and absolute last-seen step. The crab's
current memory clock is separate metadata: the passage of time must not rewrite
every distant cell. Compute age and clarity from that clock when displayed.

Keep current visibility and obstruction caches bounded to nearby chunks. Sample the
level's geometry when filling an obstruction cache; do not allocate a full-world
solid grid at character creation. Compress each changed memory chunk independently,
using runs or deltas where helpful. A character export can assemble these records
for portability, but periodic saving must not build that export every five seconds.

## Acceptance before replacing the interim saver

Prove bounded resident memory and bounded capture work as the explored archive grows.
Exercise distant terrain and memory after cache eviction and reload, interrupted
commits, reset during worker completion, denied storage, real quota exhaustion,
regional authority moves and owner departure. Compare complete memory contents and
regional replay hashes; do not treat a successful write alone as persistence proof.

No compatibility or migration layer is required during pre-alpha. Player schema 3
replaces the dense map format. Obsolete browser schemas start fresh with saving
enabled; damage in the recognized current format still protects the prior save.
Incremental commits, cold paging and bounded caches are wired into both game targets.
Browser cold-start and co-op acceptance must pass alongside the native scaling tests.
