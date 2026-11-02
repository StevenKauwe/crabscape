# Saving a growing world

The current paged saver is an interim implementation, not an unbounded world store.
It freezes the explored terrain archive and reconstructs that archive on load. Map
memory still uses arrays spanning the level and repacks remembered cells into one
base64 string. Paging the terrain payload did not make either path scale indefinitely.

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

No compatibility or migration layer is required during pre-alpha. This document
describes the next storage implementation; it does not claim that cold paging,
incremental chunk commits or sparse runtime memory are already wired into the game.
