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
- [x] Wants that send you out (#631): five pieces and recipes each miss one far thing, one shell load apiece: shed teeth (bone hall tooth beds, Shell mosaic), seep mussels (seep garden, Whelk door), a mantis telson (the mantis over the clay cathedral, x 16,700, Coral garden), a great mantis club (the mantis past the canyon, x 10,720, Culture bed 3), three moray fangs (a moray past the canyon, Moray-tooth claw tips). Creatures past x 9,000 leave a far part by where the crab eats the body (`items::drops_at`); `Sim::wants` names the landmark or the far creature. The sixgill cannot be beaten, so no want asks for one. `skills-and-resources.md`, "Wants that send you out"
- [x] Re-measure the loop on the new map (2026-11-11, #626): `cargo test --release -p crabsim --test loop_pacing -- --ignored --nocapture measure_routes` (about 25 min; `LOOP_ONLY=glass` and `LOOP_EGGS=n` narrow it), `ROUTES` pasted. The lamp's garden is now the far rim's (across the canyon), the fight is the basin's mantis; totals 5.62 / 3.48 / 2.88 min (were 4.81 / 2.05 / 2.78)
- [x] Digging earns Excavating (#631): a crab holding dig toward sediment is paid 1 XP per new 16-unit block under its claws (last 96 blocks remembered, so digging in place or in a refilled tunnel pays about nothing); the home burrow's shaft and gallery pay 206 XP / min against 180 for tier 1 and 620 for tier 3 (`economy_dig.rs`, `economy_pacing.rs`)
- [x] Tier 5's trip is measured, not extrapolated (#631): 307 s round trip to the vent field (`measure_vent_trip`), tier 5 pays 1,049 XP / min at its unlock level, still above tier 4's 805
- [x] Death wakes you at the nearest rest point you have reached (#633): home, a crevice rested in, a region entrance (far basin, lower canyon), or a boss door; `level/rest.rs` derives them (the world lane adds `MARKERS` rows), `rest_points.rs` holds the rules. A death in a boss arena (the octopus's, the grouper's) wakes you at its door with your load kept and no cast-off; an overworld death still leaves a cast-off with the load, and several wait at once
- [x] The grouper is beatable (#633): the fair bot wins 67-70% (perfect 100%), from 0%; it never gaped at a crab on its slope (aim, range and cone) and its hurtbox was out of reach until the gulp carries the crab up to its head. The sixgill stays a hunter to avoid
- [x] Silt Galleries glass for the lamp's want (#638): the three glass gardens moved (same node indices) from the doorstep and far rim into the Silt Galleries' first two sponge galleries (the third lies within a hermit crab's reach of the plateau above); the want names "upper gallery". Re-measured: the lamp loop is 2.63 min (22 s out, 12 s home), down from 5.62
- [x] The Deep Galleries' halls hold nodes (#638): fish falls (lanternfish hall), black coral (black coral hall), whale-fall bones (whale-fall hall) and nodules (the nodule field, the deep band), three each at the world lane's sites
- [x] Region 5's shelf tops hold nodes (#638): glass gardens and glass scallop beds on the three twilight tops, a sea-pen field and a clam bed on the kelp ledge; the lamp's want still names the Silt Galleries (the nearest place glass grows)

**Loop world** (map, regions, landmarks: the world lane)
- [x] Onboarding: the first-hour guide (`economy/guide.rs`): 17 small steps walk a new crab through all 13 skills in about 15 minutes of bot play, one goal on screen at a time, skippable (`skills-and-resources.md`, "The first hours")
- [ ] Way out too short: the shallows end at the first open water (the basin, 2,706 units from the door) after 14 to 17 s on foot (measured again on the Plate D map, unchanged by it). The familiar way out is a quarter of its 4 minutes
- [ ] Crossing too short: 96 s from the basin to the far rim's landing (the lamp's garden) and 12 s to the mantis mouth, against 6 minutes; the open water is still one clean line, no detour to plan, and the new canyon floor (x 7,130 to 7,610) is 46 s out, nearer than the far rim
- [ ] The fight is short: the basin's mantis (3,785 units from the door) takes a dive and a climb out (a 1 to 6 egg crab cannot get home from the pit: only the full kit can), then a 6 s scripted win. A hunting want needs a creature that is worth a crossing: the coral garden's mantis at x 16,700 is now one (the solver reaches it with the full kit: 105 s of crossing, 129 s out in all); the one at x 10,720 and the morays are not measured
- [ ] Way home too short: 100 s from the far rim, 63 s from the basin (no tired-leg wait), and no hunter between you and home
- [ ] Nothing but the lamp's garden at the far rim: nothing to find on the way (the 12 to 15 minute "strange, beautiful moment" has no place yet). The bone hall and seep garden have nodes and prizes, and wants now send a crab to each (and to the cathedral's mantis); the route solver cannot play the digs into them, so only the cathedral's mantis is measured
- [x] A first cavern near home, the Silt Galleries (region 2, issue #4's PR): a mouth on the trench floor with a rest point, the upper gallery's three glass-sponge galleries (the gatherer's line, out of every hunter's reach), the lower gallery and the moray hall (the hunting ground, a rest point at its door), a ledge over the future Drop (seen, not reached yet) (`level/silt_galleries.rs`, `tests/silt_galleries.rs`)
- [x] The shortcut home opened from the far side: a plug at the top of a shaft up to the tide-pool shelf, dug only from below, open for good after (the long chimney's duct blocks a tunnel straight into the home burrow, so it comes up under the shelf at x 920 to 1,040)
- [x] Several cast-offs wait at once (`tests/cast_offs.rs`); death and respawn rules belong to the systems lead, rest-point markers to this lane
- [x] Terrain authored as a 2D layer map (caverns under ground), baked per chunk: a region edit rebakes 2 chunks in about 26 s, not the whole map
- [x] The crust vault has a second door (a duct and shaft); the lower canyon descent has a refuge every 904 or less (was 1,946)
- [x] The world painted like its creatures: oil-painted material tiles, ink rims, painted parallax ridges that never end or drift on a dive (`painted-world.md`)
- [ ] Rest-point markers at every region entrance and boss door (octopus lair, sixgill at the kelp foot, grouper cave) in the systems lead's `level/rest.rs`, once it lands
- [ ] Map memory still draws cavern interiors as solid (`src/memory.rs` reads `floor_height`); other creature walkers too
- [ ] The plug shaft is a 1,000-unit climb with no mid-way rest
- [ ] Regions 4 (the Drop and the Deep Galleries, the octopus lair), 5 (open-canyon shelves, arches, the kelp column home) and 6 (the slope caves and the grouper) are the next big PRs; most of the map is still dead rock until then
- [ ] Re-measure the loop with the galleries: home, then the sponges, then home through the plug, against 20 minutes
- [ ] The Silt Galleries are a near loop (found by #638): the solver walks down the silt mouth to the sponge galleries in 22 s and is home in 12, so the lamp's loop fell from 5.6 to 2.6 min. The 20 minutes need the galleries' depth (the lower gallery, the moray hall, the plug's one-way dig) on the way to the glass, or the glass further in; the world lane's call

## Measured loop (2026-11-11, Plate D map after #610, one-trip piece costs)
`crates/sim/tests/loop_pacing.rs` plays one loop per want, headless, and prints this table with `--nocapture`.
Real ticks for the gathers, the fight, the kelp, the cooking and the building; the route solver (cached in `ROUTES`)
for the way out, crossing and way home, cut at the landmarks by kind; fixed UI time assumed: 60 s at home, 45 s to build,
15 s per craft. Windows are this file's beat sheet; a beat is OK between half and one and a half of its window.
The wants' targets are found by kind: the lamp's garden is the farthest glass sponge garden (since #638, the Silt
Galleries' second sponge gallery, under the trench by home), the fight is the mantis of the lowest x (the basin's).

Glass lamp (gather, tier 3): Harvesting 30, Building 8, glass cutter tier 2, 1 cord banked (the lamp needs 4 glass: one shell load); kit 6 eggs + scissors + glass + crusher + dactyls.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI + 0.2 s of commands; the board pins the lamp |
| Way out | 2-6 | 0.36 | too short | 22 s on foot down the silt mouth to the garden, 2,525 units from the door; no open water on the way |
| Crossing | 6-12 | 0.00 | n/a | no crossing: the galleries are a walk under the trench |
| Prize | 12-15 | 0.23 | too short | 4 glass at 3.5 s from 1 garden; calm and quick by design: the beat's length belongs to the getting there |
| Way home | 15-19 | 0.25 | too short | 12 s of route + 3 s walking in |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 2.63 | | (5.62 when the garden was the far rim's, before #638) |

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

Coral garden (fight, mantis telson; a want that sends you out, #631): Building 55, black coral and clay already in the bank (the garden needs one mantis telson), Fighting 1; same full kit.

| Beat | Window | Minutes | Verdict | Reason |
|---|---|---|---|---|
| Home | 0-2 | 1.00 | OK | 60 s assumed UI; the board pins the garden |
| Way out | 2-6 | 0.24 | too short | 14 s to the first open water, 2,706 units from the door |
| Crossing | 6-12 | 1.76 | too short | 105 s from the basin to the coral stand by the cathedral, 13,709 units on; 9 s more to the fighting stand |
| Prize | 12-15 | 0.24 | too short | the real fight took 6 s; the mantis is 16,605 units from the door |
| Way home | 15-19 | 1.95 | too short | 113 s of route + 4 s walking in |
| Build | 19-20 | 0.78 | OK | 45 s assumed UI + 2 s |
| Total | 20 | 5.98 | | |

What it says: the loop is 3 to 6 minutes on the Plate D map, not 20, and the gap is still the world's: the familiar
way out, the crossing and the way home are each a fraction of their windows, and the route is a few clean lines with
nothing on them. Against the pre-#610 map (4.81 / 2.05 / 2.78 min) the new map adds distance only where a want now
goes farther (the far rim, the basin's mantis); nothing on it lengthens the shallows or puts a decision on the way. The
fix is the world lane's (a longer familiar way out, a crossing with a detour to plan, a hunter on the way home, a
strange place to find on the way), not costs or slower harvests (the guardrail above). The places that could carry a
want now exist with nodes (the bone hall 3,880 to 4,130 at -2,534, the seep garden at -10,514, both dug to); no want
sends a crab there yet, and the route solver cannot play a dig.
