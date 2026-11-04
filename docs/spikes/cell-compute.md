# Cell-compute spike, November 2026

The first implemented pass reduces native simulation CPU cost by roughly
4–6x on the measured routes. A 10x whole-game improvement is not established.
Keep the draft playable so phone frame pacing and the changed background
arithmetic can be tried before landing it.

## What was expensive

* The simulation crate inherited `opt-level = "z"`. Optimizing this crate for
  speed alone reduced ordinary material ticks by about 2–2.5x in the first run.
  The rest of the game retains its size-oriented release profile.
* Each moving window allocated and initialized five replacement grids, then
  copied their overlap. They now move overlapping rows in their existing
  allocations. Resizing still allocates; equal-size movement reuses all five
  buffers (2,995,200 bytes at 640x360).
* Sleeping-page access performed `Arc::make_mut` ownership checks for each
  individual grain. A page is now detached/borrowed once before its cell loop.
* Bottom-edge preparation repeated cache lookup/locking per cell, and was
  repeated after remapping. It now looks up each page once and samples the
  new edge once.
* Cold clipped outgoing pages were discovered while executing a speculative
  full remap. The candidate was discarded and repeated after reads arrived.
  These reads now occur in preflight. Existing rollback and bounded-cache
  guards remain for falling-grain operations and unexpected cold reads.
* Background waves called software sine twice per updated water cell. The
  same trigonometric equations are separable into row and column terms.
  Column terms are pure derived data, cached for one horizontal window per
  thread. This changes floating-point rounding, not the intended tide field.
* Co-op's presentation loop immediately retried blocked ticks in the same
  browser task. It now yields after a blocked tick, as solo play already did.
  Network messages and storage reads can then arrive before the next attempt.

Water still diffuses on the fine grid in the same checkerboard order; pebbles,
mud cohesion, suspension, entrainment, wakes, and grain movement retain their
rules and order. Simulation/material rates remain 120/30 Hz. No unsafe code,
new runtime dependency, coarser material resolution, or dropped canonical
tick was introduced.

## Reproducible CPU comparison

Base: `994db8d`. Run `cell_transition_budget` in release mode on both versions.
For a source-independent authored-terrain comparison use
`CRAB_TRANSITION_RAW=1`; it changes the test map's max_x by four units to
bypass the source-fingerprinted global bake on both versions. The benchmark
still uses the same opening snapshot fallback, paging code, terrain geometry,
players and material phases. Each row below contains 30 independent samples.

These are native CPU medians, in milliseconds. They exclude browser storage
latency, drawing, uploads and network transport. They are not phone FPS.

| Route | Ordinary material tick, base → spike | Complete cold paged crossing tick, base → spike |
|---|---:|---:|
| Opening east, fresh | 3.965 → 0.764 | 6.551 → 1.447 |
| Opening return | 3.974 → 0.763 | 6.691 → 1.241 |
| Silt east, fresh | 3.487 → 0.719 | 7.774 → 1.837 |
| Silt return | 2.680 → 0.613 | 7.104 → 1.307 |
| Canyon down, fresh | 2.794 → 0.478 | 10.538 → 2.142 |
| Canyon return | 3.057 → 0.517 | 7.158 → 1.635 |

Fresh canyon crossing p95: **11.061 → 2.418 ms**. Its spike's p95 ordinary
material tick is 0.534 ms. A new-data crossing still costs substantially more
than an ordinary tick, even after this pass.

`flow_basis_spike` separately measures only background-wave generation:
approximately **26x** faster in the native experiment. It fills every grid
cell, whereas gameplay updates one checkerboard color and only water. The
microbenchmark's gain must not be reported as the complete simulation gain.

## Equivalence and limits

Before changing the wave arithmetic, `cell_compute_digest` matched four
baseline SHA-256 traces exactly: opening, silt, canyon and gravel, with 144
material steps each, returning windows, swimmer wakes and jumps. It hashes
full checkpoints, including sleeping cells, flow, grain and aggregate IDs.
This supports the bit-exact storage/copying optimizations; it does not claim
that factored floating-point arithmetic retains old replay hashes.

The factored wave tests compare the old direct equations across depth,
different origins, resizing and long clocks. Wave error is bounded separately
from local fluid evolution. Three 128x128 material fixtures run 120 real
steps with repeated wakes and jump impulses against the previous equations.
The observed material occupancy differences were zero; each material's mass
matched, and maximum flow-component differences were below 0.000001. This is
a useful bounded comparison, not a proof that every chaotic grain trajectory
is identical indefinitely. Both peers and replay runners must use the same
new version, as usual.

Focused tests also check overlapping moves in every direction, resized and
disjoint rectangles, preservation of absolute cell identity, reuse of the
original allocation, and a cold clipped page causing **no active-grid
mutation/remap** before the read completes. Existing streaming, conservation,
checkpoint, co-op and replay tests remain the integration safeguards.

The bundled settled terrain is rebaked and compacted using the new equations;
startup does not settle the world synchronously. There is no save migration.
The new column cache is not saved, hashed, networked, or authoritative and
cannot grow with explored world size.

## Next architectural pressure points

1. Measure the actual rendered phone path after these changes. Separate
   simulation CPU, cold storage waits, region move preparation/validation,
   transfer compression, texture upload and rendering in its trace.
2. The remaining crossing cost is mostly entering/leaving terrain work.
   Consider bounded staged preparation with an atomic publish, while the old
   window still runs; do not publish half-prepared terrain or mutate a held
   checkpoint. Existing isolated chunk experiments do not establish a faster
   complete gameplay replacement.
3. Rendering already uses the GPU; material stepping currently uses the CPU.
   A new GPU solver needs its own equivalence and transfer-cost evidence.
   The current fine-grain update order is part of gameplay and cannot simply
   be made independent per tile without changing it.

This spike supplies a substantially faster CPU baseline from which to judge
further storage changes, threading or a different simulation framework.
