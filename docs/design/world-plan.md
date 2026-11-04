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
4. **Farming out to Codex:** `codex exec -m gpt-6-astra -c model_reasoning_effort=low "<brief>"` run inside that
   worktree, in the background, with a time limit (`timeout 3600`). (`-m astra-low` is rejected on this account;
   astra at low effort is the same thing.) The brief is the issue body plus: "Read docs/design/world-vision-brief.md
   first. Write the failing tests first. Decide; do not ask. Do not touch files not listed."
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
7. **Human taste gates.** Each milestone below ends in a gate. At a gate, **stop and wait**: post the PR's preview
   link (`https://stevenkauwe.github.io/crabscape/pr-<n>/`) on the gate's issue with the "what to try" list and a
   recommendation, add `needs-human` to the next phase's first package, and do not start it until the owner answers.
   Do not guess at taste. Packages marked **(gate)** do not auto-merge: open the PR without `--auto` and wait.
8. **Rebakes.** Any change to the level's floor or rocks rebakes the settled-world seed (~9 minutes). Only the packages
   marked **rock batch** change rocks; nothing else may. If you need rock, add it to the next rock batch's issue.
9. **Never touch** co-op plumbing (`crates/sim/src/net*`, `src/coop*`), saves infrastructure (`save.rs`,
   `save_data.rs`, `cell_save*`), deploy (`.github/`, `tools/previews.sh`), or the cellular engine
   (`crates/sim/src/underwater.rs`, `crates/sim/src/underwater/`, `src/cellular*`) unless the issue says it is a
   **gated engine package**.

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
| W0.3 (#456) | Fear-register invariants as tests (refuge reach, no hunters in tunnels, prize in sight, exits) | M | W0.1, W0.2 | none |

Acceptance: `cargo test -p crabsim --test world_shape` proves the three registers on the existing level, and the
existing gates hold for every ability kit. Human gate: none (no feel change).

### Phase 1: the canyon overworld
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W1.1 (#457) | Depth bands as data (light, sound, grain, smell, residents per bed) and the lantern's colour and reach by band | M | W0.1 | W1.2 |
| W1.2 (#458) | The open-water hunter: the sixgill shark becomes a sim hunter with a long tell | M | W0.3 | W1.1 |
| W1.3 (#459) | **Rock batch 1:** the lower canyon (slump dam, narrows lock, vent field) plus every bedrock frame phases 3 to 5 need | M | W0.3, W1.1 | none |
| W1.4 (#460) | Keys, gate, tether and loop on the lower canyon (no rock) | M | W1.3 | W2.1 |

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
- Does the silence before the shark arrives feel like a warning? Is there anything that sounds like air or bubbles?
- Does the watched eye make you uneasy without ever being a fight?

### Phase 3: sediment pockets and burrow mouths
| ID | Package | Size | Needs | Parallel with |
|---|---|---|---|---|
| W3.0 (#464) | **Gated engine package:** the cell generator fills authored strata bodies inside bedrock frames | M | W1.3 | none |
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
| Open water: a refuge within 600 units; hunter tell at least 2.5 s | `world_shape.rs`, `open_water.rs` | W0.3, W1.2 |
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
