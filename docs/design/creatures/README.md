# Bestiary

The first enemies, one page each, with a concept sheet in `art/creatures/<name>/` (SVG source + painted PNG) and all
of them on `art/creatures/bestiary-contact-sheet.png`. World and food web: `../universe.md`.

| Creature | Role | Zone | The tell (real behaviour) |
|---|---|---|---|
| [Snapping shrimp](snapping-shrimp.md) | Turret, short-range shock | Shallows rubble | Snapper claw cocked open (~0.5 s) |
| [Mantis shrimp](mantis-shrimp.md) | Rusher, armour breaker | Shallow reef burrows | Rumbles, rears, cocks its club |
| [Snowflake moray](snowflake-moray.md) | Ambusher | Reef crevices | Pulls back into an S, mouth shuts |
| [Queen triggerfish](queen-triggerfish.md) | Tank + ranged | Reef and sand | Hovers nose-down over you, spine up |
| [Common octopus](common-octopus.md) | Boss | Rocky shallows, its den | Rises and spreads its web |
| [Alarm jelly](alarm-jelly.md) | Hazard | Midnight zone | None: a red shape on black and a drifting tentacle |

## Sizes at crab scale (drawn and hit the same; `tests/creatures.rs` checks them)
| Animal | Real adult | Game | × crab shell |
|---|---|---|---|
| Our crab (blue-crab-like swimming crab) | Females mature at 112.5–129.5 mm carapace width [HART21]; up to 23 cm [CBP] | 12 cm wide: hurtbox 72 × 48 | 1 |
| Snapping shrimp (*A. heterochaelis*) | Up to 5.5 cm long ([HS98], `snapping-shrimp.md`) | 5.4 cm: 34 × 12 | 0.45 long |
| Peacock mantis (*O. scyllarus*) | 3–18 cm long ([ADW], `mantis-shrimp.md`) | 15 cm: 94 × 22 | 1.25 long |
| Snowflake moray (*E. nebulosa*) | Common 50 cm, max 100 cm; depth 1/14–1/23 of length ([FB-EN], `snowflake-moray.md`) | 60 cm long, 3.8 cm deep; head 44 × 24 out of its crevice | 5 long, 0.3 deep |

- [HART21] Hart, Crowley & Walters (2021), "Blue crab spawning and recruitment in two Gulf Coast and two Atlantic estuaries in Florida", *Marine and Coastal Fisheries* 13(2):113–130. <https://academic.oup.com/mcf/article/13/2/113/7819956>
- [CBP] Chesapeake Bay Program, field guide: blue crab. <https://www.chesapeakebay.net/discover/field-guide/entry/blue-crab>

## Shared combat model (to build in `crates/sim`; every number tuned, lives in `Tuning`)
Times are ticks at 120 Hz. Sizes in world units (1 cm = 6.25). "Real" = measured in a source; "tuned" = chosen for play.

**The crab**
| Thing | Spec |
|---|---|
| Health ("shell") | 100. Zero = death (moult left behind, `world.md`) |
| Poise | 100; each hit's poise damage drains it; at 0 the crab staggers 40 ticks. Refills 40/s after 60 ticks unhit |
| Hurtbox | The body box (`body_half_width` 36, `body_height` 48) |
| Crusher smash | Windup 14, active 6, recovery 22. Damage 12, poise 25. Hitbox 40 × 30 beside the body on the facing side |
| Cutter jab | Windup 6, active 4, recovery 12. Damage 5, poise 8 (scissor claws: ×2 vs soft parts and arms) |
| Block (hold) | Claws up: physical damage ×0.2, poise damage still taken; stings and bites that hit the guard do no venom. Walk at 40% |
| Parry | Block pressed ≤ 10 ticks before a hit lands: no damage, attacker staggered (its own number), then a 45-tick counter window where the crusher does ×2 |
| Dodge | The existing dodge (18 ticks); invulnerable on ticks 2–13 |
| Autotomy | While grabbed: down + dodge drops a walking leg and frees the crab at once; −8% walk speed per missing leg until a growth moult completes (one-moult full restoration is a game simplification) |

**Statuses** (each one a real mechanism): *stagger* (poise broken), *stunned* (shock wave; can't act), *venom*
(damage over time + slow; only through a bite or sting that isn't blocked), *flipped* (on its back: helpless, mash
dodge to right itself, takes ×2), *grabbed* (held; mash dodge to break free or autotomise), *blinded* (in ink: enemies
that hunt by sight lose you, and the screen around the crab darkens).

**Enemy state machine skeleton**: `Idle/Patrol → Notice → Approach → Tell → Attack (active) → Recover`, plus
`Stagger`, `Flee` and `Gone`. Each page lists its states, senses, timings and boxes. Enemies live in `Sim` as a `Vec`,
step in a fixed order, and use only the seeded generator, so replays stay exact.

**Drops** are real body parts or what the animal guarded, and feed the economy in `world.md`: food for the kitchen,
materials for the three claw branches (hammer, cut, rip) and armour.

## Concept sheets
Hand-written SVG per creature (`art/creatures/<name>/<name>.svg`, side view facing left, crab from
`art/crab/crab-parts.svg` at real scale), painted like this:
1. Render the art with ink strokes hidden (`resvg --stylesheet` with `[stroke="#2A140F"]{stroke:none}` and the labels
   hidden) at 2400 px, then `crabpaint oilphoto <in> <out> --attention off`.
2. Render the full art again and keep only the ink (a luminance-to-alpha filter), plus the labels alone.
3. Stack painted fills, ink, labels (one small wrapper SVG through `resvg`), then `oxipng -o 4`.
The alarm jelly's dark panels use the strokes themselves as the ink layer instead of the luminance filter.

## The ten (the 2026-11 bestiary build)
Ten new real animals, one page each (`<name>.md`), chosen to span the depth bands and the three fear registers of
`../world-vision-brief.md`, each teaching **one** different thing. Issues: label `bestiary` (#574 scaffolding, #575 to
#584 the creatures). The sixgill (#458) and the Plunge's angler (#471) belong to the world build, not this list.

| # | Creature | Zone (today's map) | Register | The one thing it teaches |
|---|---|---|---|---|
| 1 | [Loggerhead sea turtle](loggerhead-turtle.md) (*Caretta caretta*) | Open water high over the basin | Open water | Look up: its shadow comes down from above; get under cover, its beak cracks a guard |
| 2 | [Humboldt squid](humboldt-squid.md) (*Dosidicus gigas*) | Twilight open water over the canyon shelf crossing | Open water | Read the flashing skin: the tentacles shoot far and fast; a cutter severs a held tentacle |
| 3 | [Viperfish](viperfish.md) (*Chauliodus sloani*) | Midnight midwater over the vent field | Open water | Your lantern is bait: dim it and it loses you |
| 4 | [Goliath grouper](goliath-grouper.md) (*Epinephelus itajara*) | Eastern reef, beside coral stand 20, facing west | Chamber guardian | Suction: brace and stay out of the gulp's cone, punish the slow recovery |
| 5 | [American lobster](american-lobster.md) (*Homarus americanus*) | Canyon floor at the foot of the near wall (still water) | Chamber guardian, rival | The mirror duel: it blocks and parries like you; feint and break its guard |
| 6 | [Hermit crab with anemone](hermit-crab.md) (*Dardanus* + *Calliactis*) | Basin rubble | Micro-zone rival | The shell turtle: never hit the stinging shell, hit the claws when it reaches out |
| 7 | [Bobbit worm](bobbit-worm.md) (*Eunice aphroditois*) | Two in the sand just east of the octopus den | Micro-zone ambusher | Read the antennae: don't walk into its touch; bait the snap, then punish |
| 8 | [Giant isopod](giant-isopod.md) (*Bathynomus giganteus*) | Lower canyon floor, 500 east of the whale fall | Scavenger | Armour on top: wait for it to rear and hit the belly, or turn it over |
| 9 | [Hagfish knot](hagfish.md) (*Eptatretus stoutii*) | On the whale-fall carcass (lower canyon, 600 east of the narrows lock) | Micro-zone crowd | Don't hit what slimes: the slime clogs your gills; take the prize around them |
| 10 | [Amphipod swarm](amphipod-swarm.md) (lysianassoid amphipods) | 500 west of the whale fall, dormant in the sediment | Micro-zone crowd | Wounds smell: a hurt crab draws a cloud you can't fight; keep moving or leave food |

**Every one** obeys the fairness rules: a tell from real behaviour before every blow, no one-hit kills, hunters never
in tunnels (a guardian only in its own chamber, never following into the tunnel), a refuge in sight in open water.

### How it's built (the dependency graph)
```
#574 scaffolding (Kind variants, dispatch hooks, Extra state, keeps_depth/swims, level/bestiary.rs, stub rigs + drift test, stub puppets)
  ├─ lane per creature (#575 .. #584), at most ~4 coders at once, each in its own worktree, own files only
  └─ re-place pass after the world build's geometry batch (G) merges (spots are by landmark name, so usually nothing)
```
Each creature's lane owns only: `crates/sim/src/creatures/<module>.rs` (rules, its feel numbers, `State`, `spots`,
`DROPS`), `crates/sim/tests/<name>.rs`, `src/creatures/<module>.rs` (puppet pose), `crates/crabrig/src/bestiary/<module>.rs`
(rig), `art/creatures/<name>/`, `assets/creatures/<name>-*`, and `docs/design/creatures/<name>.md`. Shared lines
(this README's table, `decisions.md`, `ocean-shaped.md` sizes) are merged by the lead. The lane deletes
`placeholder_no_bestiary_spawns_yet`'s assertion for its kind when it places it.

Pipeline for a lane: spec → tests first (tells, timings, parry windows, poise break, can be beaten, can be escaped,
never in a tunnel, no instant kill, determinism, a two-sim co-op hash) → `step`/`feels`/`phase` → rig in
`crabrig/src/bestiary/<module>.rs`, then `cargo run -p crabrig --profile art -- parts --creature <name>` and
`-- export --creature <name>` (ink only; `crates/crabrig/tests/bestiary.rs` checks drift) → pose in
`src/creatures/<module>.rs` → `spots` by landmark → checks → PR `Closes #n`, auto-merge.

### Where things stand (2026-11-11, creature lead)
Read this to continue without the lead. Check live state with `gh pr view` / `gh issue view`.
- **#574 scaffolding (PR #588):** the shared hooks. Merged before or with the ten (the ten's PR is built on it).
- **The ten (#575 to #584): all built and placed, in one PR** (branch `issue-575-bestiary-ten`), each with its spec,
  rules in `crates/sim/src/creatures/<module>.rs`, tests in `crates/sim/tests/<name>.rs`, an ink rig and a posed
  puppet. `every_bestiary_creature_is_placed_in_the_world` proves each has an encounter.
- **Where to find each in the game** (today's map, before the world build's stepped geometry): turtle high over the
  basin; hermit crab on the basin floor (x ≈ 3,250); squid over the canyon shelf crossing (≈ 6,460, −2,950); lobster on
  the canyon floor at the foot of the near wall; bobbit worms in the sand east of the octopus den (≈ 8,960 and 9,210);
  grouper on the eastern reef beside coral stand 20 (≈ 16,360); the whale fall on the lower canyon floor (carcass x ≈
  18,680, `hagfish::carcass`): amphipods 500 west, hagfish knot on it, isopod 500 east; viperfish 420 and 700 above the
  vent field.

**Hidden rules found while building**
- No creature hook into `Enemy::takes`: the lobster (guard), hermit (shell ×0.1, soft ×1.5) and isopod (armour by pose)
  correct the health `takes` booked on their next step. Works in co-op, but a blow's kill check uses raw damage. Next
  shared change: a `creatures::takes` hook (damage zones by pose and side).
- The shared grab (`fighter.grabbed`) blocks every crab action; the squid snares first (reeled in, still fighting) and
  only grips at 170 units. Only the moray frees a grab generically; a parried hold on a remote crab lasts until its timer.
- Placement constraints are tight on today's map: the grouper, hermit, isopod, hagfish and amphipods sit closer than
  400 units to a refuge (decided: allowed for guardians and crowds that never chase; `world_shape` and `rest` pass).
- Smell-drawn kinds (amphipods) have no tell without a scent: `cues.rs`'s generic-tell test skips them.
- `scenarios.rs`'s floor check exempts `Kind::keeps_depth`; `coral_rise.rs` counts only pre-bestiary foes.
- Midwater spawns keep authored depth (`keeps_depth`); compute every spot from `level.landmarks()` and
  `floor_height`, never literal y: the world build is about to drop everything east of the canyon crossing 2,000 units
  and fill the basin pit.

**Next steps, in order**
1. After the world build's geometry batch merges: run `cargo test -p crabsim --test bestiary` and each creature's
   test; re-place any spot whose landmark moved (the squid must never use pit landmarks; the lobster may move to a new
   den on the far rim if the stepped rim is wide enough).
2. Shared `creatures::takes` hook; generic grab release; a boom sound for the grouper (additive, in the music's key).
3. Economy hand-off: `DROPS` names are fixed (the economy lead maps them to Items); the hagfish's
   `prize(level)` and grouper's `prize(level, home)` give the prize positions for resource nodes.
4. Oil-painted atlases and real concept sheets only when someone asks (placeholders by decision).
