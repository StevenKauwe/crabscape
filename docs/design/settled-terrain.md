# Settled initial terrain

New places begin with sediment that has already experienced five minutes of the
actual material simulation. This is the owner's 2026-11-04 world-generation rule,
not a promise that an untouched ocean floor never moves afterward.

The offline generator builds one global grid covering the authored campaign,
with 512 world units of lower, upper and right padding. It runs 9,000 ordinary
30 Hz material steps at the same 0.35 baseline current used by the historical
opening bake. There are no independently settled pages and therefore no internal
page seams or discarded cross-page grains. Live region bounds and untrusted
checkpoint limits are unchanged; the larger grid exists only in the offline
`terrain-bake` feature. The generator asserts separate sand/mud mass conservation,
including suspended grains, and no escaped falling packets.

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

```sh
cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example bake_settled_world -- --probe 30
cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example bake_settled_world -- --write /tmp/crab-world-settled-full.postcard.deflate
cargo run -p crabsim --profile terrain-bake --features terrain-bake \
  --example compact_settled_world -- /tmp/crab-world-settled-full.postcard.deflate \
  crates/sim/assets/world-settled.postcard.deflate
```

The first command only measures cost and checks mass. The second writes the true
five-minute seed; never run it on the gameplay thread. The third explicitly
re-encodes that completed bake, verifying its existing fingerprint rather than
retagging it or simulating again. Empty-water grain bytes are unused permutation
trails: the compactor canonicalizes them and drops only identity-only water
patches. All sediment grain identity/suspension and all actual material changes
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

`crates/sim/build.rs` hashes relevant world, physics, tuning and underwater source
files at build time, normalizing path separators and CRLF to LF for cross-platform
checkouts. Runtime includes only that digest, combined with the authored
floor/rocks, material parameters and seed-format/timing constants. Source or
geometry changes invalidate the artifact and its focused regression test: rebuild
it deliberately. This conservative invalidation can require a bake for edits that
ultimately do not change sediment; it avoids silently shipping an old seed under
new physics. Loader-only presentation/tests are not part of the physics digest.

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
