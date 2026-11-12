# Settled initial terrain

New places begin with sediment that has already experienced five minutes of the
actual material simulation. This is the owner's 2026-11-04 world-generation rule,
not a promise that an untouched ocean floor never moves afterward.

The offline generator bakes the campaign **chunk by chunk**: the world (with 512
world units of lower, upper and right padding) is cut into fixed chunks of
1,024 x 1,024 cells (4,096 units), aligned to the cell grid. Each chunk is
simulated inside a window, the chunk plus a 160-cell halo all round (clipped to
the world), for 9,000 ordinary 30 Hz material steps at the same 0.35 baseline
current used by the historical opening bake, and only the chunk's own cells are
kept. Every chunk has its own **fingerprint**: a hash of the authored raster of
its whole window, the bake settings and the engine's normalised source
(`CRAB_BAKED_PHYSICS_HASH`). `bake_settled_world --write` compares each chunk's
fingerprint with the seed already on disk and rebakes only the chunks that
changed, printing which and how long each took, so a gallery edit rebakes two
chunks in under a minute and a full bake of all 24 chunks takes about two
minutes (the old whole-map bake took 300 to 800 seconds). Each window conserves
its sand and mud mass, including suspended grains, and lets no falling packet
escape. The seed is one indexed file (a record per chunk) merged into whole
columns at load.

**Seams.** The windows are separate simulations, so the same grain can settle
differently from a whole-map bake. `bake_settled_world --seams` bakes the whole
map as one grid and compares: with the 160-cell halo, 9,746 of 152,573 sediment
cells (6.4 %) differ; 4,051 of them lie within 160 cells of an interior chunk
edge and 5,695 farther in, and the count per 16-cell band is no higher at the
edges than inside, so the differences are the ordinary chaos of two long
simulations, not a visible seam. A larger halo costs bake time quadratically;
reconsider it if a seam is ever seen in play.

The initial grid includes the actual floor, authored rock, excavation sediment,
Canyon Drift shelves, shellwash bank and shallow shortcut. The global seed replaces
the old opening-only bake for fresh campaign terrain; those opening features are
not reapplied as fresh sand after settling. The historical opening snapshot stays
available for existing fixtures and custom-level fallback.

The artifact contains sorted per-column changes to material, grain identity and
suspension. It does not include a huge global water-velocity grid. Incoming water
starts with the ordinary current/tide field, then responds to the live simulation.
This is a baked sediment initial condition, not a full dormant-fluid checkpoint.
Procedural pebble aggregates retain their existing bounded local seeding rules.
Creatures are not pre-simulated by this terrain tool.

Saved active cells and archived edited pages always take precedence. The seed is
only a fallback for never-stored material; no migration, save reset or rewriting
of visited terrain is performed. Later currents, swimmer wakes, animals and digging
still disturb the bed, and existing bounded falling-sediment transport still
crosses sleeping-region boundaries. There is no new global background simulation.

## Reproduce and invalidate

The #481 integration regenerates the current strata world with the optimized
cell kernel: 9,000 steps in 290.7 seconds, conserving 202,889 grains. Verified
compaction retains 27,928 patches in 65,439 bytes. The 30-step probe took 0.965
seconds over the same 15,978,038 cells. This supersedes the artifact build timing
below; older timings remain provenance, not a controlled before/after benchmark.


W3.0 (#464) fills the home burrow, mantis mouth, clay tunnel and bone hall,
including their removed mouth closures, with authored sediment. No body below
y = -10,000 is filled; the odd tunnel and later water lanes remain deferred.
The current bounds are unchanged: the 30-step probe covers **15,978,038 cells**
(limit 16,000,000), with 87,114 sand and 115,775 mud cells. It measured 2.440 s
for 30 steps and projected 732.0 s for 9,000 steps on the development ARM64 Mac.
Pebble beds use the approved mud fallback because the bake stores scalar grains,
not aggregate ownership. Permanent frame rock is never replaced.
The final 9,000-step bake completed in **805.9 s**, conserving **202,889 grains**.
A test-only clock relocation changed the conservative source fingerprint and
required replacing the initial bake with another full simulation.
Verified compaction retained 27,928 patches in **65,438 bytes**. The 30-step digest
is `a15d93044637177b91afa30155e1df585b61c542613bbf5042a0018e60870a83`.
Generation matched 9,929 outside-frame chunk hashes against the pre-strata kernel.
The full 640 × 360 bone-hall window measured 2.223 ms p95 against a 16.667 ms
60 Hz allowance in the native development profile (not a phone measurement).

```sh
timeout 1500 cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example bake_settled_world -- --probe 30
timeout 3000 cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example bake_settled_world -- --write [path] [--jobs 3]
timeout 3000 cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example bake_settled_world -- --seams [path]
```

The first command only measures cost and checks mass over the whole map. The
second rebakes the stale chunks of the seed (the committed file by default) and
writes it back; never run it on the gameplay thread. The third measures the
seams (see above). Empty-water grain bytes are unused permutation trails: the
packer canonicalizes them and drops only identity-only water patches (this
replaces the old separate compaction step). All sediment grain identity/suspension and all actual material changes
(including emptied bed cells) remain. A real-kernel regression perturbs only water
identity bytes and verifies identical subsequent material, flow, suspension,
non-water grain state and falling packets under wakes and strong current. The development ARM64 Mac
probe used 12,773,488 cells, about 158 MiB for primary arrays, and projected roughly
nine minutes of offline computation. The completed bake took 496.5 seconds and
conserved 122,364 grains after authored opening features (raw floor alone had
122,833; the shellwash/shortcut change the initial count). Compaction reduced
804,247 patches / 2,180,227 compressed bytes to 26,552 patches / 62,753 bytes by
removing 777,695 empty-water identity-only patches. Progress is printed every
300 steps.

`crates/sim/build.rs::source_paths` hashes `src/underwater.rs`, every Rust file
recursively under `src/underwater/` except `settled_seed.rs`,
`src/physics/consts.rs`, and `src/level/coral_rise.rs`. Paths are relative to
`crates/sim`. Path separators and CRLF are normalized for cross-platform checkouts.
Edits to those files rebake every chunk. Each chunk's fingerprint combines that
source digest with the authored cells of its window (the layer map's raster,
`level/layers.rs`: the floor, sediment, strata, rocks and every authored
region), the material parameters and the seed-format/timing constants; a change
to authored terrain rebakes only the chunks whose window it touches.

`world.rs`, `tuning.rs`, `physics.rs` and `level/circulation.rs` are outside the
source digest. Adding a tuning field or a call in `Sim::tick` therefore needs no
rebake. The loader and its tests in `settled_seed.rs` are also outside it. The
30-step behavioural tripwire guards terrain changes that escape the source list.
Run it with the normal sim tests or, for a speed-optimized measurement, with
`timeout 1500 cargo test -p crabsim --profile terrain-bake --lib probe_digest_matches_committed`.
It uses the same `global` and `advance` functions as `--probe 30` on the real level,
then SHA-256 hashes row-major triples of material discriminant, grain identity and
suspension byte. Its expected digest lives in `settled_seed.rs` beside the seed
loader. A mismatch says "rebake" and prints the actual digest. For an intentional
terrain change, run all three commands above, review the changed inputs, then
replace the test's expected digest with that measured value and rerun the sim tests.
Never update the digest merely to silence an unexpected difference.

### Bake dependency trace (#475)

`bake_settled_world` dispatches to `seed_bake::{bake, probe}`. Both call `global`,
which calls `Level::test_level`, `bounds`, `raw_world`, `seed_shellwash_site` and
`seed_shallow_shortcut`. `raw_world` uses `World::new`, `TerrainColumn::{new, cell}`,
`Level::floor_height`, `terrain_with_rocks`, `initial_grain` and `ambient_current`.
`advance` calls `step_material(0.35)` and checks mass and escaped grains. Its
material/flow helpers, grain swaps, indexing, silt emission and closed-bottom
handling all live in the hashed cellular sources. The global bake has no crabs,
other bodies or pebble aggregates. `pack` compares against the same authored
columns and initial grain function and calls `settled_seed::fingerprint`.

Of the previously broadly hashed files, the reachable items are

- `world.rs` has none. The bake constructs `underwater::World`, without a `Sim`.
- `tuning.rs` has none. No `Tuning::default()` or tuning sub-struct feeds this path.
- `physics.rs` has none. `STEP` is used by creature/cell interactions outside this
  path. `STEP` and `TICKS_PER_SECOND` are moved unchanged to `physics/consts.rs`,
  re-exported at their original paths and conservatively hashed as the shared clock.
- `level/coral_rise.rs` contributes `shape` through `Level::test_level` and
  `sediment_depth` through `terrain_with_rocks`. `culture_unsealed` is unreachable.
- `level/circulation.rs` has none. Neither `circulation` nor `smooth` is called.

Level construction also calls `exploration::extend` and `hub::shape`. Their
floor/rock/max-x output is covered by the runtime geometry fingerprint. The
tripwire additionally covers changes to level sampling such as `floor_height`.

The #475 baseline and post-move 30-step digests both measured
`76a40216438979753b1f569ed67e9dbcce5e40f8a6e01420cf9a47d6f19f6d18`.
The optimized test took 5.70 seconds before and 1.84 seconds after, both below the
20-second target (builds competed for CPU during the baseline). The real-level
probe covered 12,773,488 cells. The single 9,000-step rebake took 637.2 seconds
while validation builds also ran, within its 1,500-second timeout. It conserved
63,389 sand and 58,725 mud cells, totaling 122,114. Compaction retained 25,245
patches in 59,439 bytes. Compared with the seed from main at `9a1c58e`, all
128,921 decompressed bytes outside the 32-byte fingerprint are identical.

Column lookup is allocation-free after one startup decode. Initial loading,
solo remapping, regional co-op grants and staged streaming all use the same seed.
The cell-transition benchmark must be rerun after changing this lookup; an offline
bake must not reintroduce expensive work at a live boundary.

## Traversal on the aged bed

The Rubble Banks haven keeps its existing identity but rests on a ledge anchored
in the rock cap, rather than hanging above sediment that has settled away. Its
raised floor leaves the excavated mouth and loosened-sand pile below unobstructed.
From the east, the existing four-stroke expedition budget permits a ground jump
and paddle outside the ledge before walking into the shelter; the normal and
maximum-size crab both rest and walk back out. The lower through-route remains
independent of this optional rest stop.

The excavation regression now arrives in clear water and settles naturally onto
the aged bed before performing its original digging sequence. It still proves the
tooth is collected, the rock mouth is cleared, and the crab moves beyond the cap
alive and carrying the tooth. Historical opening-save fixtures retain their old
material, flow and tick history rather than being overlaid onto the new seed.

## Measured settled-world release artifact

On the quiet development ARM64 Mac, the shipping release profile decoded and
checked the 62,753-byte artifact in 3.716 ms; constructing the fresh opening after
decode took 13.178 ms; populating all 19 habitats afterward took 0.332 ms
(single cold-start sample). Thirty boundary samples per case
covered opening-east, Silt-east and canyon-down, both fresh and archived returns.
The worst clone-plus-remap p95 was 3.950 ms, passing the opt-in 10 ms boundary-work
budget. Fresh full paged-tick medians were 6.152, 7.499 and 10.547 ms respectively;
the canyon full-tick p95 was 10.681 ms, so the full physics tick is **not** claimed
to satisfy 10 ms. These native measurements are not a universal phone guarantee.

Under the same ordinary current, supported canyon bed positions displaced during
the first five seconds fell from 73 raw to 6 baked (92% fewer), with an automated
relative bound guarding that reduction. Already suspended/unsupported grains
remain active: total changed material cells were 136 raw and 143 baked, and grains
still crossed the lower boundary. Silt's supported-bed counts were 262/277 and
Rubble's 379/353; those results are mixed, not evidence of uniformly quieter water.
The seed supplies real prior history without freezing the bed or resetting moving
grains to manufacture a zero-motion arrival.

The Canyon Drift starter valve, practice piece and outlet use the aged shelf
surfaces. Existing saved catcher state remains authoritative. Canyon regressions
retain physical support, reach, eight-second interception and inventory/ownership
requirements; crab approach fixtures start in clear water beside the bed.


The fresh Shellwash find moves to (324, 33), in the sand retained beside the
catch lip, and its valve starts at (360, 27). The former upper-slope position
(260, 43) became unsupported after aging; lowering it there would instead pin the
find to undiggable rock. The new seam stays Embedded through ordinary idle
simulation and still needs a real dig to release. Valve placement candidates now
use the bowl's actual three support points and full clearance rather than the
walking-animal footprint; the same bounded list drives the contextual UI and the
real dig/catch test. Catch speed, reach, ownership and clearance limits stay fixed.
Existing saved salvage records retain their positions.

Synthetic jump, ledge and short-step braking tests now explicitly construct their
flat/step substrate instead of accidentally inheriting the matured campaign bank.
Their original jump tolerance and step-snap bounds remain; separate canonical
opening dig, bypass and roundtrip tests continue to exercise the aged level.

Enemy spawn footing samples loaded cells first, archived edits second, and the
same baked seed only for unvisited ground, including spawns outside the active
window. Morays require clear body space above uneven footing; a valid refuge
does not rise with passing grains. A loaded refuge follows excavated support
downward, or can rise to the nearest clear height when genuinely obstructed,
within the existing bounded search. Walking enemies retain their ordinary footing placement.

## Final composed validation

The complete native `crabsim --tests --no-fail-fast` pass covered 51 targets:
987 tests passed, with one remaining failure showing that a moray refuge must
follow its supporting bank downward after excavation. The bounded lowering fix
preserves the original assertions: after 120 ticks the animal is more than eight
units below its starting height, and its home differs by less than 0.01 units.
After the necessary true rebake, the failed library target passed all 430 active
tests (34 explicitly ignored profiles/long checks), and all seven moray scenarios
passed again. Across the complete pass and focused correction, all 988 active
native simulation tests passed. Opening salvage, digging/return, the new jelly,
saved terrain precedence, replay, and regional ownership are included.

The settled-world release artifact was generated after rebasing onto the merged alarm jelly; its
midwater spawn position remains authored rather than snapped to the bed. No saved
terrain or enemy roster migration is performed. The startup/boundary figures above
refer to that 62,753-byte release artifact.

The direct downward-dive change (#288) regenerated the conservative source
fingerprint through another complete 9,000-step bake (496.1 seconds), followed by
the same verified compaction. Its artifact is 62,756 bytes, with the same 26,552
material/history patches and 122,364 conserved sediment grains. This was a real
bake, not a fingerprint retag; saved-terrain precedence and continuing dynamics
remain covered by the composed seed tests.
