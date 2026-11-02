# Cellular co-op: chunk-native simulation plan

This is a development plan, not playable co-op. The current material simulator
uses one 640 × 360 active window and a sleeping-chunk map. The experimental
host-window code can step separated regions, but copies and shifts entire
windows, does not support all player/body cases, and has no live network or
host-migration integration. Cellular co-op must stay disabled.

## Measurements and design choice

On a desktop release build, after three warmup steps, a 20-step test measured
11.30 ms per material step for one player/window and 35.08 ms for two separated
players/windows. Process peak RSS was approximately 18.3 MB and 43.7 MB,
respectively; these figures include the test harness and exclude Bevy. A
component profile of two windows averaged 0.17 ms cloning, 1.86 ms shifting to
the already active window, 7.64 ms for its material step, 7.59 ms shifting to
the far window, 7.90 ms for its material step, and 7.83 ms shifting back.
These measurements describe this desktop only. At 30 material steps a second,
two windows already exceed the 33.3 ms budget before the game renders or
handles networking.

Sending whole-world differences is also too costly. In one measured step,
2.78 MB of serialized terrain had about 955 KB of changed bytes. A compressed
XOR was approximately 1.10 MB, against 1.33 MB for the compressed checkpoint.
Those sizes and the larger buffers needed for decoding make full-world updates
unsuitable for real-time play and do not establish a safe phone-memory bound.
The join prototype also decodes a general Serde checkpoint with a 64 MiB
decompressed-byte cap; that cap alone does not bound all allocations during
deserialization or the combined peak of transfer buffers and both worlds.
The browser save path's separately bounded reader is not used for co-op joins.
Bounded parsing, peak-memory measurements and chunk-scoped backpressure are
explicit blockers before connecting this prototype to the runtime.

The first isolated chunk-native primitives reproduce the existing flow
diffusion phase, move unobstructed whole pebbles and bounded push chains across chunk seams, and
advance guarded sand and mud, including cohesion, suspension, entrainment,
and sediment wakes, in the same global order as the contiguous reference.
The sediment slice rejects pebbles and body reservations. It does not run
the full sediment simulation, pebble chains,
forces, inactive chunks or a shared game. In a 20-step release desktop test
over one full 640 × 360 region, contiguous diffusion averaged 8.54 ms/step
and the first chunked implementation 12.42 ms/step, about 45% slower. Its
chunk cell, flow, grain, suspension and owner arrays have 3,194,880 bytes of
capacity for that region, excluding flow scratch, the pebble registry,
allocation overhead and all other game state. This is a correctness baseline,
not a performance improvement or a phone measurement.

A second, still isolated diffusion path traverses contiguous rows within
each chunk and only looks up another tile at a seam. Two 20-step release
desktop runs measured 8.37/8.06 ms per contiguous step, 12.20/12.13 ms for
the first chunk implementation, and 6.68/6.70 ms for batched chunk traversal.
Both chunk paths match the contiguous reference on the tested terrain and
seams. This is a diffusion-only result, not a full-simulation speedup: it
still stages flow for the entire region and does not advance sediment,
pebbles, animals or players. No phone performance claim follows from it.

An isolated post-checkpoint command test encodes two players' interests and
forces in 97–107 bytes per material step, roughly 2.9–3.2 KB/s per receiving
guest at 30 Hz before transport overhead. It checks epoch, sequence, material
clock and an optional full-world digest; a gap or mismatch pauses the stream
and requests a fresh checkpoint. This proves only that the same prototype
inputs can reproduce the same state in the tested simulation. It is not a
networked game and does not yet capture every source of terrain change.

## State and ordering invariants

- One authoritative value exists for every world-space material cell. The
  value includes its material, flow vector, suspension, grain identity and
  pebble owner. Active and sleeping storage cannot both own the same cell.
- A 64 × 64 chunk is a storage and subscription unit, not an independent
  physics world. Pebble aggregates retain a stable world-wide owner and move
  atomically even when their footprint touches multiple chunks. Partial
  aggregate activation or movement must be rejected rather than split.
- Preserve the current phase order in `World::step_material`: compute flow
  from the prior flow field into a separate buffer; apply pebble motion in
  its existing tick-dependent owner order; then process loose grains bottom
  up, with the same alternating horizontal order and a shared moved-cell
  marker. Iterating whole chunks independently changes this order at seams.
- Use absolute world coordinates and the one material clock for cadence and
  water fields. The simulation runs at 120 Hz; material updates occur every
  fourth simulation tick. A join or correction needs both the material
  state and the proper simulation phase.
- Reserve every affected player's body during the material step. Process
  competing forces in a deterministic, host-chosen order, conserve all
  sediment and owned aggregate cells, and never displace hard rock. The
  temporary-solid host prototype is not a final fluid/body model.
- The active set is the union of all players' simulation interests, not a
  camera rectangle. A margin must cover every possible read and write,
  including fluid neighbours, sediment movement, jump wakes, the bounded
  push search, and pebble chains. Derive and test that bound from the code;
  do not assume a one-cell halo is enough. An operation reaching a sleeping
  or unsynchronized region must be deferred or expanded atomically.
- Sleeping terrain currently resumes stored state; it is not a deterministic
  fast-forward through all missed disturbances. A chunk design must state
  when an inactive region is advanced, approximated, or left unchanged.

## Steps toward an implementation

1. Extract the material state and the existing flow, pebble and grain phases
   behind an accessor. Keep the current contiguous window as the reference
   implementation. Measure allocations, bytes per active chunk, and step time
   before and after each change.
2. Implement chunk storage and a world-coordinate accessor with explicit
   ownership and halo exchange. Run the exact same phase and global row/owner
   order against one window's worth of chunks. No multiplayer or networking
   yet: every cell, flow value, grain, suspension and pebble must match the
   contiguous reference at each tick, including a negative-Y window shift.
3. Exercise seams: grains fall and drift both ways across chunk borders;
   water disturbances diffuse across them; a multi-cell pebble crosses and
   pushes a chain across a border; competing forces approach from both sides.
   Assert conservation, aggregate integrity, deterministic ordering, and the
   same result whether interests overlap or were supplied in reverse order.
   Compare each case against the contiguous reference, including after
   deactivation and reactivation. Reject cases without a correct owner/halo.
4. Only then step the union of separated interests without copying whole
   windows. Bound active chunks, scratch buffers, sleeping history, packet
   queues and checkpoint work against measured device budgets. Benchmark one,
   two nearby and two separated players on desktop and representative phones.
   The planner's current 4,096-chunk maximum is a safety ceiling, not a
   practical phone-memory target.
5. Feed an ordered host command stream into that simulator. It must account
   for *all* terrain-changing actions, not just crab controls: jumping,
   digging, wakes, animal movement and animal embedded-body clearing in
   `world.rs`, plus changes to the level and relevant combat events. Enemy
   decisions use the simulation's seeded RNG and can run on different peers
   in the existing co-op design; synchronize the resulting authoritative
   effects or the complete deterministic state and tick order. The current
   player-only command proof is insufficient.
6. Add bounded per-chunk digests and authoritative corrections at explicit
   tick barriers. A gap, invalid command, changed host, or digest mismatch
   must pause the affected stream and request a checkpoint/correction; never
   apply a partial patch or continue on a guessed state. Bind every packet to
   the selected actual transport connection, reject duplicate identity claims,
   and preserve the guest's private solo world through disconnect and host
   changes. Limit queue depth and outstanding bytes before sending.
7. Run the same native and WebAssembly traces across joins, delayed/dropped
   messages, corrections, overlapping and distant players, disconnects and
   host migration, then test with real phone browsers. Enable gameplay only
   after terrain, player movement and entity authority agree and performance
   is acceptable on the target devices.

The guarded sand slice matched the contiguous reference for six steps on
synthetic three-grain fixtures crossing both seam directions, including a
negative world-Y origin; material was conserved and replay was deterministic.
On a separate synthetic two-grain fixture, two release desktop runs of one
step repeated 20 times, including reset clones, measured 11.71/11.50 ms per
contiguous step and 10.06/9.78 ms per guarded chunked step. One test-process
peak was 28.0 MB, including both versions. This is a sparse restricted case,
not representative full-world or phone performance. The guarded step clones
the complete chunk field before advancing so it can roll back, then allocates
flow and moved-cell scratch; it is not an incremental or phone-memory design.
Additional isolated tests compare a cohesive mud cluster across a seam,
mixed sand and mud, and suspended fine mud moving upward across a seam with
the contiguous reference. These retain deterministic ordering and material
counts. Additional focused seam fixtures compare entrainment and falling-grain
wakes and silt with the contiguous phase, including negative world coordinates,
material conservation and deterministic repeated traces. The reference emits
six silt particles when fewer than 1,200 exist, so its maximum after an
emission is 1,205; the prototype preserves that cap and copies the particle
vector when cloning, adding memory beyond its reported field allocation.
These fixtures do not cover every interaction in a populated world. Two
release desktop runs of a sparse five-mud-grain one-step fixture, including
reset clones, measured 12.54/11.75 ms contiguous and 9.65/9.46 ms chunked.
One test-process peak was 28.0 MB, including both versions. These figures do
not represent a dense world, full simulation, or phone performance.

A separate transactional pebble-push primitive matches the reference's
row-major blocker discovery, leading-first aggregate movement, finite force
costs and motion impulses in tested two-aggregate seam fixtures containing
loose sand and mud. A failed push leaves the chunked state unchanged. This
prototype rejects player reservations, non-cardinal pushes, unlimited force,
and chains over 256 aggregates; its sediment step still rejects all pebbles.
It does not implement full pebble stepping, streaming, or gameplay co-op.
In one desktop release run, 20 pushes with fixture reset clones took 1.84 ms
for the contiguous reference and 5.36 ms for chunks. The rollback implementation now journals at most 65,536 cell swaps and 256
aggregate states rather than cloning the field; allocation failure or exceeding
the bound rejects the push before mutation. A later desktop run with identical
fixture resets measured 2.60 ms reference, 2.66 ms journal, and 4.91 ms with
an added full-field rollback clone for 20 pushes. This small sparse fixture is
not a phone memory or frame-time result. The chunk field itself occupies about
3.19 MB excluding particles, metadata, scratch and other copies; the journal
can add up to about 2 MiB for swaps on a 64-bit platform, plus aggregate
states and plan storage.
