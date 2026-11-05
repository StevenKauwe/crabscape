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
7. **Human taste gates.** Each milestone below ends in a gate. At a gate, **stop and wait**: post the PR's preview
   link (`https://stevenkauwe.github.io/crabscape/pr-<n>/`) on the gate's issue with the "what to try" list and a
   recommendation, add `needs-human` to the next phase's first package, and do not start it until the owner answers.
   Do not guess at taste. Packages marked **(gate)** do not auto-merge: open the PR without `--auto` and wait.
8. **Rebakes.** The settled-world seed's source fingerprint (`crates/sim/build.rs`) hashes
   `crates/sim/src/underwater.rs`, all Rust files recursively in `underwater/` except `settled_seed.rs`,
   `physics/consts.rs` and `level/coral_rise.rs`. Runtime also fingerprints the floor, rocks, maximum x,
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

## Where things stand (2026-11-05, lead designer's handoff)
Read this first if you are continuing without the lead designer. Check each item's live state with `gh pr view <n>`
or `gh issue view <n>` before acting.
- **Merged:** plan and bible (#473, #476, #486, #488), W0.1 landmarks (#474), W0.2 route library (#480), W0.3 fear
  invariants (#485), W0.4 seed fingerprint (#479), W1.1 depth bands (#478), W2.1 pressure on the body (#483), and a
  fix for main's flaky save tests (#491).
- **Queued to auto-merge:** W2.3 anxiety (#484, finishes draft #419: close #419 once #484 merges), W1.3 canyon
  refuges (#490). If either fails its checks, rerun the failed job once, then read the log.
- **At a taste gate, waiting for the owner:** W2.2 sound (#493, Milestone 2; the "what to try" list is on #462). Do
  not merge it or start anything that needs it until the owner answers on #462 or #493.
- **In flight:** W0.5 memory save size (#492, a gated saves package). If its Codex run stopped, the worktree is
  `.claude/worktrees/memory-save` on branch `issue-492-memory-save`.
- **Parked, built:** W1.3b lower canyon (#487) on branch `issue-487-lower-canyon`, rebaked. Blocked only by #492:
  a fully explored memory of the bigger world is over the 4 MiB save limit. After #492 merges: rebase, rebake if
  the seed changed on main, update the probe digest, full checks, PR, auto-merge. The handoff comment on #487 has the
  details.
- **Next, in order:** W1.2 open-water hunter (#458: needs #487 and the Hush cue from #493's gate), W1.4 keys and
  tether (#460: needs #487), then W3.0 (#464) and the rest of phase 3. Milestone 1's gate is W1.2's PR.
- **Owner-facing notes to carry:** #419 (draft) can be closed once #484 merges; the session's first Codex attempts
  are summarised in "Farming out to Codex" above.

## Hidden rules found while building (read before any package)
- **The seed fingerprint.** See item 8: only `underwater*`, `physics/consts.rs`, `level/coral_rise.rs` and the
  level's floor and rocks force a rebake. A 30-step probe digest test (`probe_digest_matches_committed`) catches
  terrain changes from anywhere else and says "rebake". Two PRs that both rebake conflict on the binary seed: the
  second rebases and bakes again.
- **The route solver is slow.** `crates/sim/tests/common/route.rs` plays real ticks. Give every survey a tight stop
  predicate (stop past the region you test); keep each test file under about 60 s on CI. Exhaustive kit matrices
  are `#[ignore]` and run by hand (`-- --ignored --nocapture`).
- **The level is a single-valued heightmap** (`Level::floor`, one height per x; rocks are boxes on top). A deep
  place can only go where the floor itself is deep: nothing can sit "under" the far rim or the reef without
  turning them into rock roofs, which breaks every animal's `floor_height`. New deep ground goes at new x: the
  lower canyon continues east of Coral Rise (x > 17,400), as the owner's map has the canyon stepping down to the
  right.
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
- Does the silence before the shark arrives feel like a warning? Is there anything that sounds like air or bubbles?
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
