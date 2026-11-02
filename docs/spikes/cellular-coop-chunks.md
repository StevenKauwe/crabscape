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

The first isolated chunk-native primitive reproduces the existing flow
diffusion phase exactly and moves one unobstructed whole pebble across chunk
seams without splitting it. It does not yet step sediment, pebble chains,
forces, inactive chunks or a shared game. In a 20-step release desktop test
over one full 640 × 360 region, contiguous diffusion averaged 8.54 ms/step
and the first chunked implementation 12.42 ms/step, about 45% slower. Its
chunk cell, flow, grain, suspension and owner arrays have 3,194,880 bytes of
capacity for that region, excluding flow scratch, the pebble registry,
allocation overhead and all other game state. This is a correctness baseline,
not a performance improvement or a phone measurement.

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
