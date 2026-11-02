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
The integrated material step handles complete in-window aggregates and one
solo body reservation, and rejects incomplete aggregate footprints. It does not run
the full gameplay simulation, player-driven pushes,
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
not representative full-world or phone performance. The guarded step used to clone the complete chunk field before advancing.
It now validates all recoverable errors before mutation, then allocates flow
and moved-cell scratch; the enclosing host operation still clones for rollback,
and this is not an incremental or phone-memory design.
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
and chains over 256 aggregates; pushes remain a separate primitive.
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

The guarded chunk material step now runs diffusion, aggregate motion and loose
sediment in the same order as the contiguous step. A generated settled level
with aggregates matched the reference for one step; a mixed sand/mud and
two-pebble seam fixture matched for six, including an impulse, wake, particle
state, aggregate ownership and deterministic replay. Incomplete/offscreen aggregate footprints reject the entire step; actual
streaming commonly requires such footprints, so this is not a full-world
solution. Two desktop release runs of ten generated-level steps including
fixture reset clones measured 97.4/92.8 ms contiguous and 95.4/98.2 ms
chunked. The chunk field occupies 3.19 MB, but both reset and transactional
step clones, flow and movement scratch, particles and the reference world add
more; peak process memory could not be measured in the sandbox. These results
do not establish phone suitability or a complete multiplayer simulation.

The material step additionally matches the solo simulator's one-body protected
area: grains cannot enter it except for the existing slide-off behavior, and
nearby aggregates may move away from the crab. Focused seam fixtures compare
sand, cohesive and suspended mud, strong-flow entrainment, and pebble impulses
over multiple ticks. This is a stationary reservation copied from one solo
world, not support for player forces, jumping, digging, multiple reservations,
or resolving overlaps. Those interactions still need explicit synchronized
semantics and parity tests before co-op can use this step.

A restricted solo force prototype now applies the reference jump downwash and
immediate entrainment and reproduces loose sand/mud digging using the same
breadth-first scoop and force costs. Direct reference tests compare jump and
dig outcomes, silt and flow, then continue material stepping across seams.
Digging supports complete aggregate footprints using the journaled push planner,
including solo body exclusion and diagonal sweep choices; it rejects force above
64 and searches needing the alternate scoop fallback, leaving state unchanged.
Jumping rejects aggregate cells in its entrainment footprint. Digging still
clones the entire chunk field to roll back, and neither operation constitutes
complete movement or multi-player force support. No game path uses them.

Solo-reference pebble digging tests now cover a seam, adjacent aggregates,
mixed mud, multiple force budgets, subsequent material steps, replay, and a
late unsupported loose-scoop failure after a pebble moved in the staged
state. The staged failure discards that movement. Incomplete/offscreen
aggregates, alternate scoop fallback and multiple bodies remain unsupported.

An executable two-Sim integration regression now joins a guest to a host
checkpoint, creates distinct local and remote crabs, applies opposing real
controls, advances both `Sim::tick` loops and delivers ordinary co-op
messages with `FakeLink`. Their actual terrain fields diverge after eight
ticks. This is an intentionally negative test and does not show playable
co-op: each game loop independently follows its local crab and modifies
terrain. The existing checkpoint/ordered-frame test advances terrain directly
and never exercises these game loops.

A genuine end-to-end test needs an explicit game-loop integration boundary:
collect each player's actual movement/terrain commands at the right fixed
tick; host-order and step all active regions exactly once; transmit ordered
authoritative state or commands with gap/resync handling; and make guests
consume that result instead of stepping a second independent local world.
Remote crab movement/collision reconciliation, enemy and environmental forces,
overlapping reservations, sleeping-region boundaries and streamed aggregate
ownership must also be addressed. The current host-window solid-body model
and whole-field clones are not substitutes for exact behavior and phone-ready
performance. Until those constraints are met the game must keep cellular
co-op disabled.

An isolated frame envelope now passes through the existing FakeLink scheduler,
which exposes its actual source device index to a test delivery callback. A
joined guest rejects another connection impersonating the host, accepts the
selected host and rejects a repeated frame as stale; rejected deliveries leave
terrain unchanged, and ambiguous duplicate host claims prevent selection.
Frame messages are bounded and validated when decoded. `Sim::receive` ignores
them, and the game does not wire this experimental delivery path: a real
transport must bind its callback to an authenticated connection and selected
host. This is transport testing, not a passing live two-player simulation.

A guarded actor-free game-loop seam now lets fresh, online Sims advance their
normal fixed ticks while an ordered, source-bound frame advances the same full
solo material-and-visual step once every four ticks. It records the settled
terrain's starting clock (currently 9,000), checks the expected cadence, and
rejects players, enemies, recording, missing frames and changed connections.
An eight-tick test joins two Sims, delivers the frames through FakeLink, and
compares both complete terrain states to a third solo Sim at every tick. This
is an exact narrow fixture, not a playable co-op mode: no player or creature
can enter this seam, it uses a full world step and digest, and it is not wired
into the game.

A further isolated spectator test captures a single real, stationary host
crab, advances the host's normal game loop, sends its body reservation via
FakeLink, and compares the guest's full terrain to a solo simulation. The
frame requires a post-step digest and rolls back on mismatch. The guest has no
crab, ordinary crab messages are discarded by this fixture, and host movement,
actions, creatures and active recording are rejected. This is one terrain
tick, not player replication, movement prediction or a playable co-op test;
full-world cloning and digesting remain unsuitable for phone gameplay.

The isolated spectator seam also supports one host crab walking horizontally:
it captures the real pre-tick controls and velocity, then reproduces the solo
body reservation, walking brush, swim wake and water stirring. A 16-game-tick
FakeLink test verifies actual crab motion and a nonzero wake, compares four
material steps to solo, and checks that a bad post-step digest rolls terrain
back. The host's ordinary crab updates are intentionally not delivered to the
guest; the guest has no player. Between material frames the host can update its
reservation and active window without a corresponding guest update, so this
does not establish off-tick collision or rendering parity. Jumping, digging,
upward motion, guest crabs and creatures are still rejected. The spectator
frame uses its wake fields for pre-tick velocity, unlike the separate host
window prototype; the two interpretations are selected by the frame shape.

A guarded two-player Sim-loop experiment now runs one host crab and one guest
crab with real horizontal controls. It sends guest force commands, ordinary
crab updates and host terrain frames through FakeLink, applies exactly one
host material step, and matches an independently ordered shared-terrain
reference at three material ticks. The fixture checks both crabs moved,
sand/mud counts, rejected unsupported host input and bad-digest rollback.
The guest crab must be ungrounded, in the same active window, separated from
the host, with its entire protected region and a movement margin containing
only water. That guard ensures a second reservation cannot affect the next
material step; overlapping bodies, sediment near the guest, jumping, digging,
creatures and host migration are unsupported. Input authentication and
sequencing are performed by the test harness, not a wired production session.
The prototype clones whole state and hashes terrain, and guest collision or
window state between material frames is not reconciled. This is a narrow
integration experiment, not playable co-op or evidence of phone performance.

An isolated chunk-material primitive now permits up to three additional
reservations alongside the solo body. It sorts additional IDs, rejects
intersecting protected regions and boundaries that cut a body margin, keeps
sand and mud out of each protected area, and applies the existing slide-off
rule to the body blocking a grain. A separated three-body fixture compares
local behavior around each body with the one-body contiguous reference,
checks seam behavior, material conservation and input-order determinism.
Pebbles, jump forces and digging are rejected while extra reservations are
present. This is not a full multi-player force model: body overlap, aggregate
contact, shared digging and the real Sim integration still need work.

A further guarded two-Sim fixture lets the guest walk on loose substrate. The
host applies the guest's ordinary walking brush and wake, restores its own
reservation, and advances the active field with both separated reservations.
The guest receives the same frame and advances its real local crab tick. In a
12-tick fixture, both crabs move, an intentionally placed grain is brushed,
the complete host and guest terrain states agree at material ticks, local
fields match independently stepped single-body references, and sediment is
conserved. Bad digests and unsupported input leave the staged state unchanged.
The fixture deliberately dissolves all pebble ownership while preserving its
sand cells: the contact path rejects any world containing aggregates. It also
rejects overlap, window-edge bodies, jumping, digging and creatures. It copies
the full field through the chunk representation and clones state for rollback,
so it is not yet suitable for live gameplay or a phone performance claim.

The experimental loose-sediment scoop can now preserve both separated body
reservations. An isolated batch sorts player commands by ID, rejects duplicate
actors and unsupported fallbacks, and rolls back all earlier scoops if a later
one fails. Each body's local result is compared with a solo scoop, including
force budget, material conservation and protection of the other body.

A separate two-Sim fixture applies actual guest horizontal dig controls on a
purpose-built pebble-free sand bed. It checks full host/guest terrain agreement
at material ticks, compares each body's local terrain to independent solo
references, and compares guest movement and nonzero dig resistance with a
separately ticking solo guest. The real-tick prototype currently orders the
host's walking action before the guest's dig; simultaneous host digging,
other directions, fallback scoops, aggregate-bearing worlds, overlapping
bodies and creatures remain unsupported. Authentication and sequencing of
guest input are still supplied by the test harness, not a live session.

A desktop release profile of the earlier grounded walking contact path took
97.0 ms for ten normal solo material ticks and 247.8 ms for ten guarded host
material ticks, including reset clones in both measurements. The guarded path
therefore averaged about 24.8 ms per material tick in this fixture, before
networking and rendering; this single desktop measurement does not establish
phone performance. The new dig path also clones fields and has not been
profiled separately. Neither path is ready for live co-op.

Removing two full-field conversions used only for reservation validation and
the redundant prevalidated chunk-step clone reduced one desktop release
fixture from 244.2 ms to 189.8 ms for ten guarded grounded-host ticks, both
including reset clones (about 24.4 to 19.0 ms per tick). Corresponding solo
times were 99.1 and 96.3 ms. This single fixture is not a phone measurement;
the host still clones its full state for rollback, converts the active field
to and from chunks, and allocates flow and movement scratch. The chunk step
has no recoverable errors after its checks; tests cover nonfinite input, tick
overflow, aggregate rejection with existing silt, and a real host brush that
mutates staged terrain before a late guest rejection restores the original.
