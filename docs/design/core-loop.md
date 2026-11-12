# The core loop: a great 20 minutes

The yardstick for the first polished stretch (home → first cavern → home) and for every region after it. If a
feature doesn't make these 20 minutes better, it waits. Hook and rules: `vision.md`.

| Minutes | Beat | What it feels like |
|---|---|---|
| 0–2 | **Home: pick a want** | Wake in the shared burrow. Something you want to build or make needs one thing you don't have (a glass-sponge lamp needs 16 glass). Cook a meal for a buff, check the task board. You leave with one clear goal. |
| 2–6 | **The familiar way out** | Known shallows, low tension. Gather on the way, pop a couple of pistol shrimp for easy loot, XP ticks up. You feel your progress: this was hard an hour ago. |
| 6–12 | **The crossing** | Open water or a dive into new depth. The stroke reserve is a resource; you already plan the way back. If you came for loot, a real fight, won on reading tells. You spot a ledge you can't reach yet: a future want. |
| 12–15 | **The prize** | You arrive and it's calm: danger guards the way, never the harvest. Take the rare thing. A strange, beautiful moment: the surreal ocean pays off here. |
| 15–19 | **The way home is the test** | Full cargo, the climb debt, a hunter between you and home. Choose the known route or the shortcut you dug or opened last time. Dying means a corpse run for your cargo, Valheim style. |
| 19–20 | **Home: change the world** | Unload, build the lamp; the base visibly changes for everyone in the session. A level, a new task or recipe, and the memory of that ledge. A natural place to stop, or "one more dive". |

## Rules
- **One clear want on the way out.** RuneScape's pull: you always know your next goal.
- **Tension rises going out and peaks on the way back.** The ocean twist; Silksong combat lives at the peaks.
- **Every trip pays in three currencies:** materials, XP and knowledge (a shortcut, a route, a place you now know).
- **It ends at home with a visible change.** Valheim: the thing friends see.
- **Danger is chosen by your goal** (owner, 2026-11-11). Even deep, a gatherer can mostly avoid danger with care:
  routes, timing, a shaded lantern, reading the hunters. If you want loot (or a creature's part, a trophy, a
  boss), combat and surviving it *is* the point of the trip. Gathering trips are about the route; hunting trips are
  about the fight.
- **The low-attention version is the same loop without the dive:** mine near home, then build.
- **Death costs a walk, not the evening** (owner, 2026-11-11). Elden Ring respawns: you wake at the nearest rest
  point (a region's entrance, the last crevice, home), and a big fight has one at its door, so a boss retry is
  instant with no trek. Valheim corpses: a mistake in the open leaves your cast-off shell and cargo where you died,
  and several can wait at once. Recovering one is always shorter than the trip out.

## What's missing for the 20 minutes
Each lead keeps their own lines here. `[x]` means it is in the game; the rest say which issue or PR carries them.

**Loop systems** (skills, resources, crafting, building: `skills-and-resources.md`)
- [x] Calm gathering nodes, near home too: 82 nodes, out of every foe's reach (#601)
- [x] Farmable creatures drop parts that stay useful at every level (#601)
- [x] Cooking buffs: ten foods, one buff at a time (#601)
- [x] Level-ups say what they open, and the new tasks that opens (#601, #617)
- [x] Home "pick a want", rules: `Sim::wants` lists recipes missing one kind of thing with where it comes from, from memory only; the Wants tab shows the board and the next unlocks (#617)
- [x] A pinned goal, rules: `pin_want`, `pinned_status`, one-shot `goal_ready` away from home, saved and replayed; a small gold-when-ready goal line on the HUD while out, and a note when it is ready (#617)
- [x] Home pieces in the shared home, rules: eight pieces, host-authoritative, saved in the host's world; a Home tab builds them (also in co-op sessions) and each is drawn on the shared home (#617)
- [x] Nodes on the new Plate D map (#572, after #610): by landmark name: tier 2 at the far basin hub (the home burrow's gallery holds none: 23.5 s of safe dig from the door, it would pay twice the hub; its own two clams are the first dig's payoff), the bone hall's floor (tier 3 fish falls and tooth beds), the seep garden (tier 4 seep mussels and sulfur mat), the vent field (tier 5); `economy/place.rs`, `skills-and-resources.md`
- [x] Guardrail (owner, via the coordinator, 2026-11-11): the 20 minutes come from the world (distance, route choice, the crossing, the way home), never from padding with costs, slower gathering or repeat trips down the same route. A piece costs about one good trip (a shell load), plus at most an optional second run. The lamp is 4 glass + 1 cord, the curtain 6 kelp + 2 cords, the bone arch 2 whale bone + 3 clay + 1 mantis club; a glass cut is the tier's 3.5 s (#622 tried 16 glass and a 30 s, then 12 s, cut and undid them)
- [x] Home pieces are wants (#622): `wants` lists a piece one kind short (not built yet), `pin_want` / `pinned_status` / `goal_ready` take piece words, and building it lets the pin go; the Wants tab shows them
- [x] Fight wants (#622): one kill, one drop; the bone arch needs one mantis club. On the Plate D map the mantis is the one at the mantis mouth in the basin, 3,785 units from the door across the basin's pit (12 s of crossing, a 6 s scripted win, 63 s home); the crab has the full kit, because its bank already holds whale bone from the canyon floor
- [x] Re-measure the loop on the new map (2026-11-11, #626): `cargo test --release -p crabsim --test loop_pacing -- --ignored --nocapture measure_routes` (about 25 min; `LOOP_ONLY=glass` and `LOOP_EGGS=n` narrow it), `ROUTES` pasted. The lamp's garden is now the far rim's (across the canyon), the fight is the basin's mantis; totals 5.62 / 3.48 / 2.88 min (were 4.81 / 2.05 / 2.78)
- [x] Digging earns Excavating (#631): a crab holding dig toward sediment is paid 1 XP per new 16-unit block under its claws (last 96 blocks remembered, so digging in place or in a refilled tunnel pays about nothing); the home burrow's shaft and gallery pay 206 XP / min against 180 for tier 1 and 620 for tier 3 (`economy_dig.rs`, `economy_pacing.rs`)
- [x] Tier 5's trip is measured, not extrapolated (#631): 307 s round trip to the vent field (`measure_vent_trip`), tier 5 pays 1,049 XP / min at its unlock level, still above tier 4's 805

**Loop world** (map, regions, landmarks: the world lane)
- [ ] Way out too short: the shallows end at the first open water (the basin, 2,706 units from the door) after 14 to 17 s on foot (measured again on the Plate D map, unchanged by it). The familiar way out is a quarter of its 4 minutes
- [ ] Crossing too short: 96 s from the basin to the far rim's landing (the lamp's garden) and 12 s to the mantis mouth, against 6 minutes; the open water is still one clean line, no detour to plan, and the new canyon floor (x 7,130 to 7,610) is 46 s out, nearer than the far rim
- [ ] The fight is short: the basin's mantis (3,785 units from the door) takes a dive and a climb out (a 1 to 6 egg crab cannot get home from the pit: only the full kit can), then a 6 s scripted win. A hunting want needs a creature that is worth a crossing; the mantises at x 10,720 and 16,700 lie beyond the solver's kit today
- [ ] Way home too short: 100 s from the far rim, 63 s from the basin (no tired-leg wait), and no hunter between you and home
- [ ] Nothing but the lamp's garden at the far rim: nothing to find on the way (the 12 to 15 minute "strange, beautiful moment" has no place yet). The bone hall, seep garden and cathedral now have nodes and prizes, but no want sends a crab there

## Measured loop (2026-11-11, Plate D map after #610, one-trip piece costs)
`crates/sim/tests/loop_pacing.rs` plays one loop per want, headless, and prints this table with `--nocapture`.
Real ticks for the gathers, the fight, the kelp, the cooking and the building; the route solver (cached in `ROUTES`)
for the way out, crossing and way home, cut at the landmarks by kind; fixed UI time assumed: 60 s at home, 45 s to build,
15 s per craft. Windows are this file's beat sheet; a beat is OK between half and one and a half of its window.
The wants' targets are found by kind: the lamp's garden is the farthest glass sponge garden (the far rim's, across the
canyon; the doorstep's two are 31 s out and a worse crossing), the fight is the mantis of the lowest x (the basin's).

Glass lamp (gather, tier 3): Harvesting 30, Building 8, glass cutter tier 2, 1 cord banked (the lamp needs 4 glass: one shell load); kit 6 eggs + scissors + glass + crusher + dactyls.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI + 0.2 s of commands; the board pins the lamp |
| Way out | 2-6 | 0.24 | too short | 14 s to the first open water, 2,706 units from the door |
| Crossing | 6-12 | 1.59 | too short | 96 s from the basin to the far rim's landing, 5,214 units on; 1 s more to the node |
| Prize | 12-15 | 0.25 | too short | 4 glass at 3.5 s from 1 garden; calm and quick by design: the beat's length belongs to the getting there |
| Way home | 15-19 | 1.74 | too short | 100 s of route + 4 s walking in; no tired-leg seconds |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 5.62 | | |

Bone arch (fight, mantis club): Building 35, whale bone and clay already in the bank (the arch needs one mantis club), Fighting 1; kit 6 eggs + scissors + glass + crusher + dactyls (the whale bone came from the canyon floor; with 1 to 6 eggs and no tools the solver finds no way home from the basin's pit).

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI; the board pins the arch |
| Way out | 2-6 | 0.28 | too short | 17 s to the first open water, 2,706 units from the door |
| Crossing | 6-12 | 0.19 | too short | 12 s from the basin to the mantis mouth's door, 889 units on |
| Prize | 12-15 | 0.10 | too short | the real fight took 6 s; the mantis is 3,785 units from the door |
| Way home | 15-19 | 1.12 | too short | 63 s of route + 4 s walking in |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 3.48 | | |

Kelp curtain (the low-attention loop): Building 3, 6 kelp plus 2 cords (10 kelp in all) from the pockets beside the door, no tools.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI; the board pins the curtain |
| Way out | 2-6 | 0.00 | n/a | the kelp is beside the door |
| Crossing | 6-12 | 0.00 | n/a | none |
| Prize | 12-15 | 0.60 | too short | 10 kelp at 2.5 s over 2 stands (x 232 and 442), 10 s of walking and tucking in (the low-attention loop is meant to be short and repeatable) |
| Way home | 15-19 | 0.00 | n/a | in the prize beat |
| Build | 19-20 | 1.28 | OK | 75 s assumed UI (two cords) + 2 s |
| Total | 20 | 2.88 | | |

What it says: the loop is 3 to 6 minutes on the Plate D map, not 20, and the gap is still the world's: the familiar
way out, the crossing and the way home are each a fraction of their windows, and the route is a few clean lines with
nothing on them. Against the pre-#610 map (4.81 / 2.05 / 2.78 min) the new map adds distance only where a want now
goes farther (the far rim, the basin's mantis); nothing on it lengthens the shallows or puts a decision on the way. The
fix is the world lane's (a longer familiar way out, a crossing with a detour to plan, a hunter on the way home, a
strange place to find on the way), not costs or slower harvests (the guardrail above). The places that could carry a
want now exist with nodes (the bone hall 3,880 to 4,130 at -2,534, the seep garden at -10,514, both dug to); no want
sends a crab there yet, and the route solver cannot play a dig.
