# Cell-compute spike, November 2026

Ordinary simulation CPU cost falls roughly 4–6x on the measured routes;
the complete co-op window-move path falls roughly 7–9x. A 10x whole-game
improvement is not established.
It landed without waiting for a phone play-test: it only removes work, the
tide change is below anything a player could see (material occupancy matched
exactly in the fixtures), and the playable preview stays the next check.

## Download cost

Compiling `crabsim` at `opt-level = 3` instead of `"z"` makes the game's
WebAssembly larger. CI's web build on the same base (`2e1b1d3`):
`crabscape_bg.wasm` 16,097 KiB (6,327 KiB gzipped) before, 16,837 KiB
(6,585 KiB gzipped) after: **+740 KiB, +258 KiB gzipped (about 4%)**. Judged
worth it for a 4-6x faster simulation on phone CPUs; the rest of the game
keeps its size-tuned profile.

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
* The lobby archive's strip loader constructed the entire standalone demo
  before discarding and replacing its grids. It now allocates an empty kernel
  at the requested dimensions. The standalone demo's unchanged terrain is
  sampled once per column when it is explicitly requested.
* Region grants, move application and commits used scalar archive access:
  each grain repeated the map/cache/COW work. Rectangle access now borrows
  each page once. Active-array cells take precedence, clipped pages preserve
  their other cells, and fresh terrain reuses per-column authored geometry.

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

`cell_coop_transition_budget` additionally exercises the actual canonical
archive, granted kernel and ordered prepare/apply/encode/finish move protocol,
using 384x384 regions. It separately reports each phase, complete move time
and an SHA-256 over all encoded move outputs. The opening, far gravel and
deep fixtures produce exactly the same move bytes as the base revision; this
is a storage/constructor change, independent of the tide reassociation.

Matched sequential native runs, 20 samples per case (milliseconds):

| Co-op route | Grant, base → spike | Complete prepare/apply/encode/finish, base → spike |
|---|---:|---:|
| Opening | 5.135 → 0.598 | 20.733 → 2.988 |
| Far gravel | 11.850 → 1.091 | 21.670 → 2.336 |
| Deep | 13.986 → 1.010 | 20.708 → 2.352 |

The complete move includes encoding/compression and commit validation. It
excludes grant/extraction setup, network travel, disk latency and rendering.
Both versions hash identical encoded move outputs for all three cases.

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

After all 9,000 steps of the full five-minute global bake, the compacted
sediment payload also matches the base exactly: 128,920 bytes, SHA-256
`62a1d6e2f33107b9562f0310fc9c459872ef7ea76d6014fd7b938585458515db`,
excluding the source fingerprint. This covers material/grain/suspension and
bake metadata; the seed does not store water velocity. The new main's
independent 30-step material/grain/suspension tripwire also passes unchanged.

Focused tests also check overlapping moves in every direction, resized and
disjoint rectangles, preservation of absolute cell identity, reuse of the
original allocation, and a cold clipped page causing **no active-grid
mutation/remap** before the read completes. Existing streaming, conservation,
checkpoint, co-op and replay tests remain the integration safeguards.
Bulk archive tests compare every read and edit against the scalar reference
across unaligned active/sleeping seams and negative Y, with held snapshots
sharing the original pages. The scalar mutator is now test-only.

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

The optional headless browser profiler records all frame intervals and the
WebGL renderer. Its default SwiftShader acceptance mode is software rendering
and cannot establish hardware FPS. Use `CRABSCAPE_PROFILE_GPU=1` for local
hardware profiling; both modes use their own disposable browser profiles.

The final local hardware run used ANGLE's Metal renderer on an Apple M2 Max.
Both clients recorded 765 frames during co-op travel: median/p95 intervals
16.7/16.7 ms, maximum 33.4 ms, and no gap above 60 ms. The window revision
advanced repeatedly, with browser region moves around 4–5 ms median and
7.5 ms worst. Both browser replays reproduced natively; separation, digging,
reunion and owner-loss/private-world restoration also passed.

There is still a **150–250 ms simulation pause at shared-to-separated lobby
topology change**. The canvas continues rendering while the stop/checkpoint/
grant handshake completes. This is distinct from ordinary same-authority
window moves and needs a separate pipelined authority-transition design; the
CPU gains do not eliminate that protocol barrier. No phone-FPS claim follows
from the local Metal result.

CI also exposed parallel native save tests invalidating each other's global
reset epoch. The damaged/unreadable/closed-passage load tests now take the
same test lock as in-flight-write/reset tests. Save behavior and assertions
remain unchanged; this fixes test isolation rather than retrying a flaky run.
