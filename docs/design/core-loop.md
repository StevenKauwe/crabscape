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
- [ ] Nodes on the new Plate D map (#572, after #610)
- [x] Guardrail (owner, via the coordinator, 2026-11-11): the 20 minutes come from the world (distance, route choice, the crossing, the way home), never from padding with costs, slower gathering or repeat trips down the same route. A piece costs about one good trip (a shell load), plus at most an optional second run. The lamp is 4 glass + 1 cord, the curtain 6 kelp + 2 cords, the bone arch 2 whale bone + 3 clay + 1 mantis club; a glass cut is the tier's 3.5 s (#622 tried 16 glass and a 30 s, then 12 s, cut and undid them)
- [x] Home pieces are wants (#622): `wants` lists a piece one kind short (not built yet), `pin_want` / `pinned_status` / `goal_ready` take piece words, and building it lets the pin go; the Wants tab shows them
- [x] Fight wants (#622): one kill, one drop; the bone arch needs one mantis club. The reachable mantis is 1,025 units (7 s) from the door: that is the world lane's line below
- [ ] Re-measure the loop on the new map: `cargo test --release -p crabsim --test loop_pacing -- --ignored --nocapture measure_routes` (about 25 min), paste `ROUTES`

**Loop world** (map, regions, landmarks: the world lane)
- [ ] Way out too short: the shallows end at the first open water (the basin, 2,706 units from the door) after 14 s on foot. The familiar way out is a quarter of its 4 minutes
- [ ] Crossing too short: 79 s from the basin to the canyon doorstep against 6 minutes; the open water is one clean line, no detour to plan
- [ ] The fight is on the doorstep: the only mantis a young crab can reach is 1,025 units from the door (7 s on foot, then a 6 s scripted win). A hunting want needs a creature that is worth a crossing
- [ ] Way home too short: 67 s from the doorstep (no tired-leg wait), and no hunter between you and home
- [ ] Nothing but the lamp's garden at the doorstep: nothing to find on the way (the 12 to 15 minute "strange, beautiful moment" has no place yet)

## Measured loop (2026-11-11, today's map, one-trip piece costs)
`crates/sim/tests/loop_pacing.rs` plays one loop per want, headless, and prints this table with `--nocapture`.
Real ticks for the gathers, the fight, the kelp, the cooking and the building; the route solver (cached in `ROUTES`)
for the way out, crossing and way home, cut at the landmarks by kind; fixed UI time assumed: 60 s at home, 45 s to build,
15 s per craft. Windows are this file's beat sheet; a beat is OK between half and one and a half of its window.

Glass lamp (gather, tier 3): Harvesting 30, Building 8, glass cutter tier 2, 1 cord banked (the lamp needs 4 glass: one shell load); kit 6 eggs + scissors + glass + crusher + dactyls.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI + 0.2 s of commands; the board pins the lamp |
| Way out | 2-6 | 0.24 | too short | 14 s to the first open water, 2,706 units from the door |
| Crossing | 6-12 | 1.31 | too short | 79 s from the basin to the doorstep shelter, 2,642 units on; 3 s more to the node |
| Prize | 12-15 | 0.29 | too short | 4 glass at 3.5 s from 1 garden; calm and quick by design: the beat's length belongs to the getting there |
| Way home | 15-19 | 1.19 | too short | 67 s of route + 4 s walking in; no tired-leg seconds |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 4.81 | | |

Bone arch (fight, mantis club): Building 35, whale bone and clay already in the bank (the arch needs one mantis club), Fighting 1; kit 1 egg.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI; the board pins the arch |
| Way out | 2-6 | 0.11 | too short | 7 s on foot to the fighting stand, 835 units; no open water |
| Crossing | 6-12 | 0.00 | n/a | none |
| Prize | 12-15 | 0.10 | too short | the real fight took 6 s; the mantis is 1,025 units from the door |
| Way home | 15-19 | 0.05 | too short | 3 s walking in |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 2.05 | | |

Kelp curtain (the low-attention loop): Building 3, 6 kelp plus 2 cords (10 kelp in all) from the pockets beside the door, no tools.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI; the board pins the curtain |
| Way out | 2-6 | 0.00 | n/a | the kelp is beside the door |
| Crossing | 6-12 | 0.00 | n/a | none |
| Prize | 12-15 | 0.49 | too short | 10 kelp at 2.5 s over 2 stands, 4 s of walking and tucking in (the low-attention loop is meant to be short and repeatable) |
| Way home | 15-19 | 0.00 | n/a | in the prize beat |
| Build | 19-20 | 1.28 | OK | 75 s assumed UI (two cords) + 2 s |
| Total | 20 | 2.78 | | |

What it says: the loop is 2 to 5 minutes today, not 20, and the gap is the world's: the familiar way out, the crossing and the way home are each a fraction of their windows, and the route is a few clean lines with nothing on them. The fix is the new map and caverns (the Loop world lines), not costs or slower harvests (the guardrail above). Re-measure after #610 and the cavern map land.
