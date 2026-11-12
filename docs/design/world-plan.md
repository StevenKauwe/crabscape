# World plan: the canyon and its strata

The build plan for the owner's chosen world: a great submarine canyon (the overworld) with sediment strata in its
soft banks (pockets behind burrow mouths, joined by seep chimneys and dug tunnels), and a secret set piece, the
Plunge. **The taste bible is `world-vision-brief.md`: read it in full before every package.** Each package below is
one GitHub issue (label `world-plan`) that stands on its own.

## How to continue this without the lead designer
1. **Order of work.** Phases run in order 0 → 5; inside a phase, the table says what can run in parallel. A package is
   **ready** when every package in its "Needs" column is merged on `main` and it has no `needs-human` label.
2. **Pick the next one.** `gh issue list --label world-plan --label claude-ready --state open`; take the lowest-numbered
   ready package whose files no running package touches. Comment "Taking this" on the issue.
3. **One worktree per package**, cut from the latest `origin/main`:
   `git fetch origin && git worktree add .claude/worktrees/<slug> -b issue-<n>-<slug> origin/main`. Never work in the
   main checkout. **At most 3 packages at once, never two that touch the same file** (each issue lists its files).
4. **Farming out to Codex.** The exact command that works on this account (`-m astra-low` is rejected: "not
   supported when using Codex with a ChatGPT account"; astra at low effort is the same thing):
   ```sh
   L=.claude/worktrees/_codex-logs; W=.claude/worktrees/<slug>
   timeout 3600 codex exec -m gpt-6-astra -c model_reasoning_effort=low -c approval_policy=never \
     -s workspace-write -c sandbox_workspace_write.network_access=true \
     --add-dir ~/.cargo --add-dir ~/Library/Caches/Mozilla.sccache -C $W - < $L/brief_<n>.txt > $L/codex_<n>.log 2>&1
   ```
   Run it in the background. The brief is the issue body, then "package notes" (what already exists on `main`, the
   exact files, the decisions the issue leaves open), then the common rules (read the brief first, tests first,
   decide, only the listed files, no commit, the checks to run). Lessons from the first runs:
   - **Keep briefs and logs in `.claude/worktrees/_codex-logs/`**, never under `/tmp`: Codex tidies "scratch
     files" and deleted a whole temp folder of briefs and logs once. Say "do not delete anything outside this
     worktree" in every brief.
   - Codex cannot `git fetch`, commit or push in its sandbox. The reviewer commits, rebases and opens the PR.
   - Codex cannot take screenshots (no GPU in the sandbox). Visual changes get their look checked at the taste
     gates, or by the reviewer.
   - A run takes 15 to 60 minutes. Codex sometimes stops early with only part of the package done and no summary:
     check the worktree against the issue's test list, and start a second run with "STATE: already done ... WHAT
     IS LEFT ..." rather than starting over.
   - Codex resolves merge conflicts well if the brief says which behaviours to keep.
   - Every diff so far needed a small fix: a stored field that should have been derived, an unused import, a
     double-applied darkening, a slow test. Read every diff.
5. **Review checklist for every diff** (whoever opens the PR does all of it):
   - The issue's **taste check**: answer every question yes, and every numeric target in range (print the numbers in
     the PR).
   - Nothing on the anti-pattern list in the brief; no full-frame filter; no air or bubbles; true sizes; biology
     sourced or "not verified".
   - Rules in `crates/sim`, presentation in `src/`; nothing changes a remote crab; no `unwrap()` in `crates/sim`.
   - `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
     `cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`.
   - Co-op: the package's co-op test passes (`crates/sim/tests/coop.rs` style: two sims fed the same commands reach
     the same hash); if it touches presentation of other crabs, `cargo cooptest --only join`.
   - Feel numbers in `scenarios.rs` unchanged unless the issue says to change them.
   - Then PR (`Closes #n`), `gh pr merge <n> --squash --auto --delete-branch`, and remove the worktree once merged.
6. **Notes** go on the issue (a comment): what was done, what is left, numbers measured. Decisions go in
   `docs/design/decisions.md` (one row, one-line reason). Never leave notes only in a chat or a private page.
7. **Human taste gates.** (Superseded 2026-11-10: post the gate, never wait; see the lanes section.) Each milestone below ends in a gate. At a gate, **stop and wait**: post the PR's preview
   link (`https://stevenkauwe.github.io/crabscape/pr-<n>/`) on the gate's issue with the "what to try" list and a
   recommendation, add `needs-human` to the next phase's first package, and do not start it until the owner answers.
   Do not guess at taste. Packages marked **(gate)** do not auto-merge: open the PR without `--auto` and wait.
8. **Rebakes.** The settled-world seed's source fingerprint (`crates/sim/build.rs`) hashes
   `crates/sim/src/underwater.rs`, all Rust files recursively in `underwater/` except `settled_seed.rs`,
   `physics/consts.rs` and `level/coral_rise.rs`, after normalising each (comments, whitespace and
   `#[cfg(test)]` items dropped: `crates/sim/build_support/fingerprint.rs`), so comments, tests and lint-only
   edits no longer force a bake. Runtime also fingerprints the floor, rocks, maximum x,
   material parameters and seed-format/timing constants. Changes to these inputs require the ~9-minute bake
   (`docs/design/settled-terrain.md`). `world.rs`, `tuning.rs`, `physics.rs`, `level/circulation.rs` and
   loader/test-only edits to `settled_seed.rs` do not force one. A field added to `Tuning` or a call added to
   `Sim::tick` leaves the seed current. The real-level 30-step probe digest test catches terrain behaviour
   changes outside the source list and requires a deliberate rebake. Batch terrain edits and bake once per
   batch in the background. Only packages marked **rock batch** change rocks; add needed rock to the next
   rock batch's issue.
9. **Never touch** co-op plumbing (`crates/sim/src/net*`, `src/coop*`), saves infrastructure (`save.rs`,
   `save_data.rs`, `cell_save*`), deploy (`.github/`, `tools/previews.sh`), or the cellular engine
   (`crates/sim/src/underwater.rs`, `crates/sim/src/underwater/`, `src/cellular*`) unless the issue says it is a
   **gated engine package**.

## Where things stand (2026-11-11, lead designer)
Read this first if you are continuing without the lead designer. Check each item's live state with `gh pr view <n>`.
- **On main:** the plan and bible, W0.x, W1.1, W1.3, W1.3b, W1.4, W2.1, W2.2 sound (#493), W2.3, W3.0, the tunnel rules
  (#586: no hunter enters a tunnel; an ambusher reaches only its body length into a mouth), the seep chimneys and the
  long chimney home with act three (#589), the ten creatures from the creature team (#599) and the economy (#596).
- **The big Plate D PR #610** (branch `issue-466-geometry`; closes #465, #466, #470, #471, #472, #458), one bake:
  - The world steps down to the right: the basin pit filled (its egg 4, clue and tooth now the bone hall's prize), the
    canyon floor raised to -6,000, everything east of the canyon 2,000 lower, the lower canyon (-10,000) deepest.
  - Five sediment bodies with doors: home burrow (two silt shafts, a clay gallery, clam beds; round trip 23.5 s and
    0.09 reserve), mantis mouth → clay tunnel → bone hall (+ seep shaft out), clay cathedral and pebble warren (+ the
    cathedral seep to the hanging garden on the dam's face), seep garden and chimney hall, crust vault and warm seam;
    the long chimney's crust plug; the Plunge's frames open.
  - The whole Plunge (odd tunnel tells after act three, the dig, the lip, the 30 s fall, the crack, the angler scene,
    the wild light as an economy item, the answer, the 20 s vent home), proven on the real map.
  - The sixgill open-water hunter (long tell from 1,200+, refuge always wins, never kills; libm trig; sharks sleep far
    from crabs).
  - The seed fingerprint now ignores comments, whitespace and tests (only real terrain or physics changes force a bake).
- **#587 the bone hall's clock** (sulfide pulses, 1.5 s hiss, scavengers): green except the King's castle browser
  check, which is red on main too (#607, fixed in #609 by another session). Rerun after #609.
- **Taste gates (posted, never waited on):** Milestone 1 (#458), 3 (#467), 4 (#471): the preview link and the "what
  to try" lists from this plan go on each issue once the PR's preview is live.

### Next, in order
1. Merge the big PR and #587 once main's castle check is green (rerun their failed jobs).
2. Post the three taste-gate previews; fold the owner's feedback in as it comes.
3. Lane A (atmosphere per bed: smells, sounds and lantern for the new beds; the three Plunge hints' sound) and lane V
   (route-solver proofs over the new map with chimneys and digs: no gate bypass, every pocket's exits, round trips).
4. Not built from Plate D: the second and cold canyons and the abyssal fan; the dug-and-shored tunnel from the clam
   beds to the mantis burrow is left to the player.

### How the built map differs from the owner's Plate D (and what it costs)
Plate D draws the strata as layered banks running under the canyon's walls, with pockets embedded in them, mouths as
dark holes in the banks, chimneys rising through the layers, and the Plunge under an abyssal fan that joins three
canyons. The game adapts the level that already exists, so:
- **Pockets are boxes, not banks.** The level is a single-valued heightmap, so the strata exist only inside sealed
  bedrock frames (four today) placed under the existing floor. Between them the ground is the generator's sand and
  bedrock, not continuous layers. Cost: you cannot read the strata as a band from the overworld; each pocket has to
  sell its own layers.
- **Doors are shafts.** The home burrow sits 460 units under home (below the cellular opening's window, whose tests
  need their ground), so its mouths are vertical shafts up to the floor instead of a hole in the side of a bank.
  Cost: going in is a dig down, not a walk in; the first dig must pay off fast (a clam in the shaft).
- **Deep ground only at new x.** Nothing can sit under the far rim or the reef, so the lower canyon (slump dam,
  narrows lock, vent field) continues east of Coral Rise (x > 17,400) instead of below the canyon, and the long
  chimney's return runs as one flat duct at y -12,700 under the whole world. Cost: the wormholes are long straight
  ducts, not winding seams, and the long chimney is a long ride rather than a surprising shortcut.
- **One canyon, no fan.** Only the home canyon exists; the second canyon, the cold canyon and the abyssal fan are
  not built, and the Plunge drops below the vent field (x about 18,700). Cost: "each canyon is a chapter" is not yet
  true; the Plunge's hint of more lies under the one canyon.
- **The bake is bounded** (16 million cells, no strata below y -10,000 except sealed frames), so depth beyond the
  Plunge stays a hint, as Plate D also intends.

## The Plate D build: dependency graph and lanes (2026-11-10, owner's orders)
The owner's orders (2026-11-10): build the whole Plate D world in one push, **never wait on taste gates** (post the
preview and a "what to try" list on the gate's issue, then keep building and fold feedback in later), and run many
lanes at once. Claude Sonnet writes the package code, Claude Haiku the mechanical work, the lead writes briefs,
reviews every diff and orders merges. Briefs live in `.claude/worktrees/_briefs/` (not committed).

### The one serial bottleneck: the geometry batch (G)
Every terrain change (floor, rocks, frames, strata, shafts, galleries) goes in **one** batch and **one** bake, merged
first. Everything else is rules, cues and drawing that find places by **landmark name** (`Level::landmarks()`,
`Level::pockets()`), never by literal coordinates, so they run in parallel before and after G lands.

The geometry batch reshapes the strip towards Plate D (decision in `decisions.md`, 2026-11-10):
- **The world steps down to the right.** Everything east of the canyon crossing (the far rim, the octopus den, the
  eastern reef, Coral Rise, the slump dam) drops 2,000 units, so each step lies below the last; the lower canyon
  (narrows lock, vent field) stays at y -10,000 as the deepest step.
- **The basin pit is filled** to the mudbank floor (y -700): Plate D's mudbank holds the mantis burrow and the bone hall
  in its bank, not a shaft. The pit's finds move into that pocket chain (egg 4, the clue and the tooth become the bone
  hall's prize area); the basin floor's net becomes the scissor gate to the hub from either side.
- **The canyon floor rises** from y -9,400 to about -6,000: the crossing stays (open water, the shark's ground) but
  the deepest place is now the lower canyon, as in the picture.
- **Five sediment bodies** like Plate D's: home burrow and clam beds (A, B), mantis burrow and bone hall (C, D), clay
  cathedral and pebble warren (E, F) under the slump dam's step, seep garden and chimney hall (G, H) under the vent
  field, crust vault and warm seam (I, J) beyond the narrows; each pocket opens to traversable ground by its mouths.
- **Wormhole shells:** the seep shaft (bone hall up to the hanging garden), the long seep (bone hall to the seep
  garden), the long chimney (chimney hall to the tide-pool shelf), the Plunge's odd tunnel, shaft, crack and stage.

### Lanes
| Lane | Packages | Needs | Files it owns | Coder |
|---|---|---|---|---|
| R: tunnel rules | W3.1 rules (#465) | none | `enemy.rs` sanctuary, `navigation.rs`, `level/strata.rs` pockets, `tests/pockets.rs` | lead |
| G: geometry batch | Plate D geometry, home burrow shafts and gallery, W3.2 terrain (#466), all frames, one bake | R | `level.rs`, `level/*`, the settled seed and digest, coordinate tests, `src/` drawing tied to places | Sonnet (+ Haiku for test transcription) |
| H: open-water hunter | W1.2 (#458) | R | `enemy.rs` (a new kind), new `tests/open_water.rs`, `src/deep.rs` | Sonnet |
| C: chambers | W3.3 bone hall clock, scavengers, prize (#467); W3.2 clam beds' finds (#466) | R | new `bone_hall.rs`, `tests/bone_hall.rs`, new `src/bone_hall.rs` | Sonnet |
| W: chimneys | W4.1 seep chimney, W4.2 long chimney and act 3 (#468, #469); the shared forced-motion ride | none | new `ride.rs`, `chimney.rs`, `tests/chimneys.rs`, `src/chimney.rs` | Sonnet |
| P: the Plunge | W5.1 to W5.3 (#470 to #472) | W (the ride) | new `plunge.rs`, `tests/plunge.rs`, `src/plunge.rs` | Sonnet |
| A: atmosphere | per-bed smells, sounds and lantern for the new beds; the three Plunge hints | G | `level/bands.rs`, `src/sound.rs` hooks | Sonnet |
| V: proofs | route solver over the new map: no gate bypass by digging or chimneys, every pocket's exits, refuge spacing, round trips, co-op two-sim hashes | G, W | `tests/world_shape.rs`, `dig_gates.rs`, `dive_budget.rs` | Sonnet |
| D: docs and checks | plan upkeep, issue text, preview checks | any | `docs/`, issues | Haiku |

```
R ──> G ──> A
│     ├───> V <── W ──> P
├──> H
└──> C
```
Merge order: R, then H, C and W as they finish (rules only, no bake), then G (the bake; rebase it last-but-one if
others merged), then P, A and V. Shared touch points (`world.rs`'s tick, `lib.rs` module lists, `decisions.md`) are
one-line edits; the lead resolves them at merge.

### Gates (posted, never waited on)
Milestone 1 (W1.2), Milestone 3 (W3.3) and Milestone 4 (W5.2): the PR gets its preview link and the "what to try"
list from this plan on the issue, with a recommendation, and auto-merges like any other.

## Hidden rules found while building (read before any package)
- **The seed fingerprint is per chunk.** The settled seed is baked in 24 fixed chunks of 1,024 x 1,024 cells, each
  with its own fingerprint (the authored cells of the chunk and a 160-cell halo, the bake settings and the engine's
  source: `underwater*`, `physics/consts.rs`, `level/coral_rise.rs`). `bake_settled_world --write` rebakes only the
  chunks whose fingerprint changed (two chunks in about 30 seconds for a gallery edit; all 24 in about two
  minutes), and `bundled_seed_is_current_and_chunk_edges_share_one_history` names a stale chunk. A 30-step probe
  digest test (`probe_digest_matches_committed`) still catches terrain changes from anywhere else. Two PRs that
  both rebake conflict on the binary seed: the second rebases and bakes again (a quick job now).
- **The route solver is slow.** `crates/sim/tests/common/route.rs` plays real ticks. Give every survey a tight stop
  predicate (stop past the region you test); keep each test file under about 60 s on CI. Exhaustive kit matrices
  are `#[ignore]` and run by hand (`-- --ignored --nocapture`).
- **Terrain is a 2D layer map** (`level/layers.rs`; was a single-valued heightmap, `Level::floor`, with box rocks on
  top). New regions are authored as shapes (cavities, soil, bedrock linings: `level/silt_galleries.rs` is the
  model); `Level::floor_height(x)` stays the ground surface for open-water creatures, and `solid_at`,
  `ground_below` and `footing` answer for caverns. Soil never hangs over open water or stands in a vertical face
  (the 9,000-step settling slumps it): line every cavity with bedrock and keep only flat beds of soil on floors.
- **Bedrock frames may hold substrate.** The cell generator fills any empty space below the floor with sediment;
  a frame only has to be sealed by bedrock (unreachable, undiggable through), not empty. W3.0 replaces the
  substrate with layered strata.
- **A rock batch updates the probe digest.** After its bake, a rock package updates the committed digest in
  `probe_digest_matches_committed` (`crates/sim/src/underwater/settled_seed.rs`); that one constant is in scope
  for rock batches.
- **Canyon refuges** are seven short coral clumps (`crates/sim/src/level/refuges.rs`), spaced by the full-kit
  rise (about 1,020 units) so they are never a ladder (`refuges_make_no_new_climb`). New refuges follow the same
  spacing rule.
- **CI flake:** two `cell_save::native` tests fail on most Linux runs and pass locally (#482). Rerun the failed job
  (`gh run rerun <id> --failed`) and add to #482; do not change saves code inside a world package.
- **`Level` derives `PartialEq` and serde:** never add derived or static data to it as a field (it breaks saves'
  round trip); add a method that derives it (`Level::landmarks()`, `Level::bands()`).
- **Frames and new rock must keep clear of existing passages.** The tests that walk a passage with the biggest crab
  (`scenarios.rs`, `canyon.rs`, `glass.rs`, `hub.rs`) catch an intrusion; known tight spots: the crack (x 5,371 to
  5,640), the pit shaft's coral (x 4,420 to 4,490), the end wall's bare-rock face from the doorstep to the notch
  (x 5,640 to 5,664), and the cellular opening window (x 0 to 2,560, above y -360). Run them before a bake: they do
  not need the seed, so a collision shows up in seconds instead of after a 12-minute bake.
- **Brittle rocks, cultures and other level lists are found by place in tests,** never as `.last()`: later packages
  append to them.
- **The bake is bounded at 16,000,000 cells** (the lower canyon brought it to about 16 million). Grow the world
  sideways in new canyons rather than deeper under everything; check `--probe 30` (it prints the cell count) before
  laying big frames.
- **Remote crabs** show no scars yet: their appearance travels over net code that world packages must not touch.

## What already exists (build on it, do not redo)
The current level (`crates/sim/src/level.rs`, `level/`) is already the upper half of the canyon: home shell and
shallows, trench and cliff, basin and pit, the far-basin hub (`level/hub.rs`), the crack down to the canyon (doorstep,
shelf, tide, upwelling, sea-pen floor, far-wall cave), the far rim's octopus den and the eastern reef. Mapping the
owner's map onto it:

| Map place | In the game now | Plan |
|---|---|---|
| 1 Home shell, A Home burrow | Home shell | Add the home burrow (W3.2) |
| 2 Tide-pool shelf, B Clam beds | Shallows | Clam beds inside the home burrow (W3.2); long chimney lands here (W4.2) |
| 3 Shelf break | Trench and cliff | Unchanged (one-way drop) |
| 4 Mudbank floor, C Mantis burrow | Basin and pit | Mantis at the burrow mouth (W3.3) |
| 5 Overhang cave, 6 Coral wall | Crack, doorstep, shelf | Unchanged |
| D Bone hall | none | First chamber (W3.3) |
| 7 Slump dam, 9 Narrows lock, 10 Vent field | none (sea-pen floor is the bottom) | Lower canyon (W1.3) |
| 8 Hanging garden | none | A coral ledge high on the canyon's near wall (W1.3); the bone hall's seep exits here (W4.1) |
| E Clay cathedral, G Seep garden, H Chimney hall | none | W4.1, W4.2 |
| 11 The pit (abyssal), I Crust vault, J Warm seam | none | Left as the hint below; not in this plan |

The dive-cost work (#452, #449: the climb home costs the reserve; the surface glide is closed; a route solver in
`crates/sim/tests/dive_budget.rs`) is the foundation of phase 0. Do not contradict it; reuse its solver.

**Act 3** (used by the Plunge) means: the long chimney home is open (W4.2 done for this crab).

## Phases and packages
Sizes follow the repo: S about 30 minutes, M about an hour. "Rock batch" = changes level rock (rebake).

### Phase 0: fear and dive-cost foundations
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W0.1 (#454) | Landmarks: every place tagged with its fear register, as level data | M | #452 merged | W0.2 |
| W0.2 (#455) | Route solver as a shared test library (per ability kit, with wormholes) | S | #452 merged | W0.1 |
| W0.3 (#456) | Fear-register invariants as tests (refuge reach, no hunters in tunnels, prize in sight, exits) | M | W0.1, W0.2 | W0.4 |
| W0.4 (#475) | **Gated engine package:** narrow the seed fingerprint so rule changes stop forcing a rebake | M | #452 merged | W0.3 |

W0.4 comes first after #452: almost every later package edits `world.rs` or `tuning.rs`, and W2.1 (#461) is parked
on it with its code pushed on branch `issue-461-strain`. Acceptance: `cargo test -p crabsim --test world_shape` proves the three registers on the existing level, and the
existing gates hold for every ability kit. Human gate: none (no feel change).

### Phase 1: the canyon overworld
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W1.1 (#457) | Depth bands as data (light, sound, grain, smell, residents per bed) and the lantern's colour and reach by band | M | W0.1 | W1.2 |
| W1.2 (#458) | The open-water hunter: the sixgill shark becomes a sim hunter with a long tell | M | W0.3, W1.3 (refuges) | W1.4 |
| W1.3 (#459) | The open-water refuges W0.3 found missing (coral clumps and crevices: data, no rock if possible) | M | W0.3, W1.1 | W2.2 |
| W1.3b (#487) | **Rock batch 1:** the lower canyon east of Coral Rise (slump dam, narrows lock, vent field) and every bedrock frame phases 3 to 5 need | M | W1.3 | W1.2 |
| W1.4 (#460) | Keys, gate, tether and loop on the lower canyon (no rock) | M | W1.3b | W2.1 |

**Milestone 1 gate: the first open-water crossing (W1.2 PR is a gate).** A human plays the preview and judges:
- Cross the canyon from the doorstep to the far rim. Does the open water feel exposed? Is the shark's warning
  readable in time? Could you always see somewhere to run?
- Sink from the shallows to the canyon floor. Does each band feel deeper (lantern colour, sound)?
- Get hit by the shark once. Did it feel brutal but fair, not cheap?

### Phase 2: atmosphere, driven from sim state
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W2.1 (#461) | Pressure on the body: strain, shell cracks that last until the moult, limb tremor (animation only) | M | W1.1 | W1.4, W2.3 |
| W2.2 (#462) | Sound hooks: sim cues (silence before arrival, sourceless groans, own-body loudness), no bubbles | M | W1.1, W2.1 | W2.3 |
| W2.3 (#463) | Anxiety: wounds carry scent, the lantern as a liability (finish PR #419), the watched eye | M | W1.1 | W2.1, W2.2 |

**Milestone 2 gate: the deep's atmosphere (W2.2 PR is a gate).**
- Rest at the canyon floor below your adapted depth. Do the cracks and the shell ticking make you want to leave?
- Does the crab still feel heavy but never sluggish when cracked?
- Does the swell before the shark arrives feel like a warning? Is there anything that sounds like air or bubbles?
- Does the watched eye make you uneasy without ever being a fight?

### Phase 3: sediment pockets and burrow mouths
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W3.0 (#464) | **Gated engine package:** the cell generator fills authored strata bodies inside bedrock frames | M | W1.3b | none |
| W3.1 (#465) | Pockets and burrow mouths as level data and rules; hunters cannot enter tunnels | M | W3.0 | none |
| W3.2 (#466) | The home burrow and its clam beds: the first burrow (teach) | M | W3.1 | W3.3 |
| W3.3 (#467) | The mantis's burrow mouth and the bone hall, the first chamber | M | W3.1 | W3.2 |

**Milestone 3 gate: the first burrow and the first chamber (W3.3 PR is a gate).**
- Dig into the home burrow. Does it feel safe and cozy compared with the open water?
- Approach the mantis's mouth. Is the ambusher readable from the ground's tells?
- Stand at the bone hall's door. Can you see the prize and the threat? Do you want it? Can you always back out?

### Phase 4: the long chimney and wormholes
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W4.1 (#468) | Seep chimneys: free one-way rides from pockets to far exits (bone hall up to the hanging garden) | S | W3.3 | W4.2 |
| W4.2 (#469) | The long chimney home (chimney hall to the tide-pool shelf), opened late; act 3 | M | W3.2, W4.1 | none |

Acceptance: reachability per ability kit with wormholes, no early access to a locked place; every pocket's exits
within a bounded length; the deepest round trip within budget once the chimney is open. Human gate: folded into
milestone 4.

### Phase 5: the Plunge
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W5.1 (#470) | The odd tunnel, the hole and the ride (a forced-motion ride in the sim; no death) | M | W4.2, W2.1 | none |
| W5.2 (#471) | The crack, the angler scene, the treasure and "something answers" | M | W5.1 | W5.3 |
| W5.3 (#472) | The vent home and the three hints (early glow, mid carcass, late draught) | S | W5.1 | W5.2 |

**Milestone 4 gate: the Plunge (W5.2 PR is a gate).**
- Find the odd tunnel without being told. Did curiosity pull you in?
- During the ride, did you feel the depth (counter, lantern colour, sound, the carcass's size)? Did you ever fear
  dying, and was that fear empty (no damage)?
- In the crack: did you hold still? Did the blue-green light feel like a treasure? Did the answering light leave you
  wanting more?

## Tests the plan relies on (all headless, in `crates/sim/tests/`)
| Promise | Test file | Package |
|---|---|---|
| Lock-and-key reachability per ability kit (overworld and wormholes together) | `route.rs` (shared), `world_shape.rs` | W0.2, W0.3, W4.2 |
| A dig never bypasses a gate | `dig_gates.rs` (extend) | W3.1 |
| Every pocket has two or more exits within a bounded length | `world_shape.rs` | W0.3, W3.1 |
| Tunnels contain no wandering hunters | `world_shape.rs` | W0.3, W3.1 |
| Open water: a visible refuge within 600 (narrow) or 1,500 (wide crossings); hunter tell covers the swim to it | `world_shape.rs`, `open_water.rs` | W0.3, W1.2 |
| Chamber prize visible from the door within 800 units; retreat within 6 s | `world_shape.rs` | W0.3, W3.3 |
| Round-trip budgets | `dive_budget.rs` | W1.4, W4.2 |
| Strain never changes movement | `strain.rs` | W2.1 |
| The ride always ends at the landing and never damages | `plunge.rs` | W5.1 |
| Determinism and co-op consistency | each package's tests, `coop.rs`, `properties.rs` | all |
| Frame-time budget of a pocket's cells | existing cell budget tests | W3.0 |

## Why this order
Foundations first (tags and tests), so every later package is checked against the fear registers by a machine, not a
reviewer's taste. The overworld comes before the strata because the strata live in its banks, and the one rock batch
(W1.3) lays every frame at once to pay the rebake once. Atmosphere comes before pockets because "the tunnel feels
safer" only reads once the open water feels dangerous. The Plunge comes last: it is the payoff of everything else.
