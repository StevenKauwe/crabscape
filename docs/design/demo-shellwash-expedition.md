# CrabScape: The Shellwash Expedition

Design lead revision, 3 November 2026. The campaign, late pursuits and timing estimates below are proposed. PR #355 contains the opening shell interaction, prepared food and an opening shortcut at the inspected integration revision e03432c. The implementation section identifies the new work required for return play.

## Decision

Build a **5–7-hour playable expedition in a small, increasingly interconnected stretch of ocean**. Its promise: **“I can dig that out, make something out of it, and use it to pull off something clever.”** The player becomes an accomplished scavenger, excavator, fighter and route-maker. The finish is reaching and understanding the living lights and establishing a reliable route home. After the ending, the opened Works becomes a sorting yard, the canyon offers drift interception and a shallow-rim fighting circuit, and the meadow becomes a place to compose and share illuminated routes. Each uses persistent terrain and a different set of player skills. A completed collection is a supported starting condition for all three districts.

The first 20–30 minutes are a validation milestone, not the demo's content target. Earlier feats, faster XP and another tooth-to-net tutorial do not deliver this design. Keep existing skill thresholds and moult duration while building it.

The three interlocking activity families are:

1. **Excavate and salvage:** read partially exposed shapes; decide where to dig, what to preserve, and where a released find will go. Catch it, chase it, or deliberately crush it for building material.
2. **Make and use field tools:** position a collecting shell, shape a catch basin, choose a digging profile, fit existing combat/traversal tools, and turn a route into something you own through your alterations. Crafting earns its place by immediately changing an activity.
3. **Pull off raids and expeditions:** extract a find from a defended or hazardous place through combat, terrain, timing or another route. Threats and moving material interfere with the extraction rather than sitting beside it as unrelated decoration.

No quest giver asks for ten clams. Your objectives are things visible in the world: an unusually beautiful shell wedged in a bank, a net hanging above a useful passage, a tooth inside a defended rubble pocket, a glittering sponge skeleton across a descending current, blue lights beyond a crack.

## Research basis and its limits

Ryan, Rigby and Przybylski's four studies associate perceived autonomy and competence with enjoyment; their multiplayer study also supports relatedness. Design implication: several viable approaches, legible improvement through execution, and optional cooperation. It does **not** establish that these particular mechanics are fun. [Primary paper](https://selfdeterminationtheory.org/SDT/documents/2006_RyanRigbyPrzybylski_MandE.pdf).

MDA distinguishes rules from the behavior they produce and the experience players have. Here, terrain removal + moving finds + limited catch position should produce anticipation, improvisation and “that worked!” moments. Counting pickups or adding an XP bar would not establish those dynamics. Iterate against actual play. [Hunicke, LeBlanc and Zubek](https://users.cs.northwestern.edu/~hunicke/MDA.pdf).

Cleveland's GDC session overview explicitly discusses exploration, discovery, emotion in prototypes, crafting and structure that does not over-direct the player. Our application is visible opportunities and equipment that changes where and how you go. Only the published session overview was inspected; no claim to have watched the talk. [GDC 2019](https://www.gdcvault.com/play/1025745/The-Design-of-Subnautica).

Michael Booth describes retaining recognizable Left 4 Dead environments while varying populations and the pacing of encounters. The design inference here is to keep the player's altered places familiar and vary what arrives, where a creature is engaged and which line is currently attractive. This talk describes Valve's game. Its results do not predict whether players will enjoy these three pursuits. [Valve GDC slides](https://cdn.fastly.steamstatic.com/apps/valve/2009/GDC2009_ReplayableCooperativeGameDesign_Left4Dead.pdf).

All numerical values below are proposed playtest settings, not biological measurements. No new animal behavior is asserted. Use existing sourced fauna only within appropriate habitat. Crafted implements follow the game's existing found-object equipment convention; a crab manufacturing a tray, scoop or sled is a game conceit, not a biological claim. Do not add magical sonar, pressure armor, underwater fire or glowing mineral ore.

## The smallest FUN slice — implement this first

One sand-bank encounter, about two camera widths across, within 15 seconds of home. It includes:

- A recognizable ribbed empty shell visibly protruding from sand. Sediment conceals the rest of its silhouette; it is a real object, not a loot roll triggered by digging nearby.
- A loose collecting valve beside a natural rock catch basin. It can be lifted and set down at a chosen reachable position.
- The player can dig above/alongside the shell to expose it and collect it safely; undercut its support to release it into a short current wash; or intentionally smash the exposed shell into two pieces of grit.
- The current carries a released shell toward a visible lower basin. A well-positioned valve catches it; a missed shell remains recoverable downstream. No permanent loss and no death in this first site.
- An intact shell can be placed on a home display ledge, used in the later collector upgrade, or explicitly crushed for grit. The choice is not made by walking across it.
- Digging itself NEVER damages a find. Only a deliberate targeted smash does. The normal movement/dig controls must remain enjoyable and forgiving.

**The slice is not complete until a player can cause the release, anticipate the trajectory, reposition/catch or recover it, and see their chosen outcome.** Exposure-aware stationary pickups alone are insufficient. A new map full of those pickups is insufficient. Ship this as one playable feature with presentation, input, save/replay and two-player behavior, not a sequence of invisible infrastructure PRs.

First playtest question: does the player spontaneously try a different extraction or catch position? If they merely hold Dig until a toast appears, fix the interaction and geometry before authoring 60 sites. Do not compensate with XP rewards.

## First five minutes: an example, not a mandatory tutorial

0:00–0:30: leave the shell home. A protruding ribbed shell and moving sediment are visible in the same view. A loose collecting valve rests nearby. One local hint reads “Dig around it. Smash it for grit.”

0:30–1:30: dig a side trench. Sand slumps into it; more of the shell becomes visible. Removing the last supporting sediment makes it drop and tumble toward the basin. The player can intercept it directly or chase it into the safe basin.

1:30–2:30: set the valve under a second exposed pocket and release a small shell/grit find into it. The catch makes a distinct clack, remains physically visible, and the crab can take it. Repositioning should take seconds, not opening a construction menu.

2:30–3:30: look right: a snapping shrimp cocks its claw beside a ribbed bank. Above is a kelp return route; below is a thin sandy approach to the same bank. Choose to approach, try the lower route, or keep excavating the safe pocket.

3:30–5:00: use a shell/grit find immediately or return briefly to display the intact shell and fit a collector or existing clam guards if materials permit. The home view shows what changed. Returning is a choice; the player does not need to make a delivery to keep playing.

## First 20–30 minutes: the first little adventure

A player has solved the first wash, tried a guarded bank, found a hidden lower entry by excavation, and acquired a tooth through one of several routes. They can fit existing scissor claws, improve their collecting setup, or prioritize guards. They cut the low net crossing to reconnect the loop, then see a larger shell perched over a wash at the district capstone.

One possible story: “I couldn't get past the shrimp, so I dug around the bank. The nice shell slid out the wrong way. I put my catch valve below it, came back through the tunnel, and cut the net to get both of us out.” Another player fights for the direct route and crushes ordinary shells to build the home sooner. Both have concrete plans for the next outing.

No feat or moult is required for these interactions. Existing feats may become available through ordinary play later, and improve optional routes. Do not bypass skill requirements merely to show a feat early.

## Core rules and feedback

### Excavation and prospecting

Finds have a stable identity, world position, collision bounds, material appearance and a small authored support/exposure sample mask. They become visible as actual sediment is removed. Do not show an opaque silhouette through solid terrain; use a visible edge, a gap, or displaced sediment to invite investigation. Every first-use site has an unmistakable clue. Later sites can reward reading substrate and nearby broken shells.

Exposure and support are separate. A find may be visible but held by sediment, collectible from an opened side, or free to move once support is removed. Support queries read the real CA cells around/below its body. A short hysteresis prevents single-grain flicker. Pickups require a reachable exposed face and a clear local path, not distance through a wall.

Three excavation methods use existing controls: dig beside a prize and collect it; undercut and intercept it downhill; cut a channel into a basin and release several loose objects. Narrow controlled digging always remains available. A crafted scoop changes sweep shape to wide/shallow, trading precision for moving more loose sediment. It does not break rock or increase combat damage.

No invisible “excavation progress” bar. Feedback is material moving, exposed object area, support movement, audible scrape/clack, and a small nearby action label. Fully concealed objects are not random loot from shovel swings.

### Free objects and collecting shells

Use small deterministic kinematic bodies, not a general rigid-body engine. Gravity/buoyancy tuning, sampled local water velocity, drag, terrain collision and settling produce movement. Dense empty shell pieces mainly roll/slide; do not make all shells float like balloons. Exact motion parameters are gameplay approximations.

Collectors have a visible mouth, limited capacity and a supported placement footprint. A free find crossing that mouth at a valid speed/size is retained. Bad placement visibly misses. Objects blocked by solid terrain cannot be magnetically collected. A caught find remains visible in the bowl.

The initial loose valve is a shared field object. The craftable collector is one deployable per player. Deploy/retrieve/empty uses one contextual action. Place only within claw reach on legal support; show the actual placement outline before confirmation. No telekinetic remote retrieval. Do not overlap crabs, doors or essential exits. Do not let placement seal a single exit.

An unbraced placed collector can become Free if support is removed or actual sampled flow exceeds its tuned holding resistance for a short sustained interval. It then uses the same bounded kinematic-body integration as a find, keeping its contents as attached references, not separately duplicated bodies. The brace increases that holding resistance only on valid supported placement; it does not anchor a collector in open water. A collector is a gameplay catcher, not a watertight dam or particle filter. Sediment flows around it through the existing system. The first implementation need only collide/catch find bodies; it must not claim to simulate a new fluid-pressure system.

### Intentional processing and trophies

At an exposed empty shell, context distinguishes “Take intact” and “Crush to 2 grit.” A deliberate smash on the shell performs the latter with a visible crack and physical fragments. Combat swings aimed at an enemy should not silently destroy a unique nearby trophy: unique display specimens require explicit targeted processing through the contextual action.

Display shells have authored recognizable appearances. Seeded rare cosmetic variations are allowed as optional surprises, but no required equipment or ending depends on rolling one. Twelve designs across the demo, two per district, no completion bonus required for progression. Show collected objects on the actual home ledge; do not turn the reward into only a checklist. Display placement is reversible. Ordinary duplicate shells can become tools or grit.

### Raids, terrain and animals

Reuse snapping shrimp, mantis shrimp and morays where their existing species/habitat supports them. Three approaches must work in appropriate raid sites:

- **Fight:** read existing tells; block, parry where allowed, counter or evade; secure the direct extraction position.
- **Excavate:** open a lower/side approach under real rock cover. Sediment alone does not confer an invented stealth buff. Solid terrain must block attacks where geometry requires it.
- **Exploit an opportunity:** an existing moray hunting a shrimp temporarily changes which bank is safe. The player can watch, enter via another route or fight. Do not require the predator event or invent a command that controls animals.

Different approaches cost different things: combat risks health but preserves a convenient bank; tunnelling takes execution and can dislodge the prize; waiting for an animal opportunity is optional because another activity is nearby. There is always another active approach, never “stand still for the timed window.”

Every dangerous extraction offers a visible retreat, a nearby safe crevice where appropriate, and a recoverable downstream landing area. Wakes may move loose sediment through existing CA behavior; new content must not invent a global noise/aggro system unless separately implemented and tested.

## Five-to-seven-hour content contract

This is an authored content budget, not a promise that every player will take exactly five hours. Skilled players can finish faster. We do not force duration with quotas, detours, slow XP, cooldowns or rare drops.

**Release inventory: six districts, 60 named local encounters, six multi-stage capstones, and 18 optional mastery challenges.** A local encounter is a problem with a different useful decision, solution or terrain interaction; moving the same clam behind another mound does not qualify.

Provisional engaged-time budget:

| Content | Available | Expected engaged time | Typical first expedition |
|---|---:|---:|---:|
| Local encounters | 60 | mean 4 min = 240 min | about 48 = 192 min |
| District capstones | 6 | mean 12 min = 72 min | 72 min |
| Optional mastery challenges | 18 | mean 6 min = 108 min | about 8 = 48 min |
| Total meaningful content | 84 activities | about 420 min / 7 h | about 312 min / 5.2 h |

These categories are disjoint: capstone sub-stages are not also counted among the 60 local encounters, and mastery challenges use distinct authored setups, not “repeat that exact encounter with a stopwatch.” Ordinary travel, menus, walking cargo home, idle waiting, death-run repetition and material farming are excluded. Short links and return shortcuts can add elapsed time but do not fill a content shortfall. Count first-use learning and deliberate problem-solving attempts; do not inflate estimates with forced failures.

Of the 60 local encounters, the first four per district introduce or materially extend a problem (24); the other six combine known verbs differently (36). Approximately 96 minutes of new grammar, 144 minutes of authored remixes, 72 minutes of capstones, and up to 108 minutes of optional mastery are available. If observed medians are two minutes rather than four, the release lacks roughly two hours: add substantive encounters or deepen decisions, do not change XP.

### District map and progression

Use the existing map's broad spaces and depth progression; do not expand storage/world infrastructure for this feature. Exact coordinates require layout validation against current CA geometry. Districts are interconnected pockets of 2–4 screens each, with vertical loops and a recognizable silhouette. Local opportunities lie 5–15 seconds apart. Home-to-farthest useful return shortcut should not routinely exceed 60 seconds of uneventful movement.

```text
HOME — A Shellwash — B Rubble Court — C Kelp Galleries
  |          |          |               |
  +-- low return loop ---+       D Lost-Net Works
                                       |
                                E Canyon Switchbacks
                                       |
                                F Living-Lights Basin
```

A↔B↔C provide parallel early routes. D has a longer natural entrance as well as a scissor-opened shortcut. E is initially descendable with existing routes/strokes; glass grips improve optional return routes after recovering a sponge. F is reachable through a safe existing upwelling/ledge structure, with harder shortcuts and extraction challenges. Required routes must not assume a particular feat or compulsory grind. Existing eggs remain concrete traversal rewards, not invisible quest keys.

Reserve 30 of 60 authored encounters, three of six capstones, and nine of 18 mastery challenges for D–F. At the current estimates each half contains 210 minutes of available authored play. A typical 312-minute first expedition should draw roughly 156 engaged minutes from each half. Measure the actual split. The return systems add zero minutes to that estimate, even when a tester voluntarily spends an evening there.

The hour bands below are a possible first run, not mandatory order. Returning to an earlier district with tools reveals previously visible opportunities; map memory marks only what the crab actually saw.

### A. Shellwash — roughly first 45–60 minutes including optional play

Identity: shallow sand, safe physical experimentation, shells accumulating in basins. New grammar: exposure/support, crush-or-preserve, collector placement, wide/narrow dig.

Ten local encounters:
1. **Ribbed Bank:** the smallest slice described above.
2. **Two Mouths:** a buried fork drains to two basins; choose outlet before releasing three finds.
3. **Sand Umbrella:** a prize rests above the intended approach; remove its side restraint before its support.
4. **Clam and Valve:** extract food and choose guards versus collector at home; both have a visible nearby use.
5. **Wrong-Way Wash:** downstream collection is safer than chasing upstream.
6. **Under the Lip:** notice a narrow gap and investigate a concealed rock nook from the side. Its distinctive geometry and view reveal a useful future sheltered approach; discovering that place, rather than collecting an item, is the reward.
7. **Full Bowl:** collect ordinary grit first or leave collector space for the larger intact specimen.
8. **Shallow Shortcut:** excavation reconnects two known banks permanently without a reward quota.
9. **Pebble Plug:** move pebbles with the existing directional sweep to open a low extraction route.
10. **Make It Fit:** a field-kit work surface beside a safe practice bank offers the actual scoop/collector recipes. Choose one from supplied ordinary material, fit it and solve its separate test setup; freely compare with the default method. This is hands-on equipment use, not time credited for reading a menu.

Capstone **The Shell Spill:** three distinct prizes at different heights. Open an exit, stage the catcher, release in a chosen order, improvise when the second release changes the bed. All can be recovered after a mistake. Reward: a distinctive intact bowl shell suitable for display or the larger collector, and the low return connection.

Three optional challenges: extract a high shell without dropping it into the lower pit; route two simultaneous releases into different natural basins; recover a lodged specimen while preserving a narrow walkable ridge. No strict countdown or mandatory perfect run.

### B. Rubble Court — roughly hour 1–2, overlaps A/C

Identity: shrimp burrows and rock cover, combat and excavation interfere. New grammar: defended extraction, attack occlusion, protected approach, predator opportunity.

Ten local encounters:
1. **Claw at the Door:** direct snapping-shrimp timing or a sandy side entrance.
2. **Low Ceiling:** jump is unavailable in the direct lane; dodge timing or dig around.
3. **Mantis Balcony:** secure the exposed face by combat or reach below solid overhang.
4. **The Hunting Eel:** observe an existing moray/shrimp interaction and choose when/where to enter.
5. **Spoil in the Escape:** bad excavation fills a retreat dip; clear the return before releasing the shell.
6. **Two Burrows:** approach direction determines which shrimp is engaged; never require simultaneous unfair attacks.
7. **Stolen Cover:** a loose find occupies the safest fighting position; extract or move it first.
8. **Mantis Rematch:** a distinct mantis encounter with an elevated retreat and two fighting positions. Its purpose is practicing counter timing, earning Fighting growth and securing an existing body meal; no guarded treasure is needed.
9. **Outfit for the Court:** prepare health or reserve portions at home from available food, then test that limited loadout on its own short adjacent recovery route. Decide when to eat and what carry space to preserve. This route is distinct from the counted combat rooms; walking home and menu time are excluded.
10. **Tooth in the Rubble:** personal progression tooth cache accessible through combat or terrain route, yielding scissor claws.

Capstone **Hold the Court:** choose how to approach three authored, separated confrontations: a snapping-shrimp crossing, a mantis holding the main landing and a moray lane with solid retreat cover. No simultaneous wave-spawning arena. Victory is reaching and securing the far safe crevice through combat, protected excavation or bypasses, then opening the court-to-home connection; no object delivery is involved. A combat-focused player can beat each confrontation for normal Fighting growth and existing corpse meals. A cautious player can reach the same refuge without exterminating its inhabitants. Record each distinctly overcome confrontation once; never require a kill quota. Reward: a usable forward refuge and route, the tooth opportunity if missed, and a visible home memento for the optional complete combat clear. Do not falsely mark returning animals permanently dead; refuge safety uses existing tucked-in protection and geometry.

Three optional challenges: bypass a two-threat court without attacking; retrieve two finds from opposite sides without sealing a retreat; win a mantis encounter on terrain with limited standing positions. They award cosmetic specimens/common material, not exclusive required power.

### C. Kelp Galleries — roughly hour 2, can precede most of B

Identity: overhead roots, rocky shelves, vertical recovery. New grammar: release from above, collector staging below, route planning under existing stroke limits. Soft kelp climb and existing rests are central.

Ten local encounters:
1. **Overhead Find:** climb, expose, then descend to the chosen catch point.
2. **Three Rest Ledges:** choose a route that leaves enough strokes for the final reach.
3. **Buried Footing:** uncover the base of a useful climb from below.
4. **Down First:** drop to a lower basin, open its return, then commit to extraction.
5. **Branch Catch:** position a collector on one of two ledges before releasing from above.
6. **Mud Curtain:** clear sight to the next landing while retaining a safe ledge.
7. **The Hanging Tooth:** an alternate personal tooth source, accessible by movement rather than combat.
8. **The Free Ride:** use an existing moving-water lane and steer onto a shelf; no added swim height.
9. **The Forward Bench:** install a field-cache module at the sheltered gallery crevice, choose prepared food versus tool space, then use that loadout on a short adjacent return test. Placement here changes the next outing; supplies are available locally and no delivery quota is attached.
10. **Look Back:** an earlier visible prize becomes reachable through the newly connected return route.

Capstone **The High Road:** choose a kelp ascent or a rock-and-current ascent, excavate two useful landings, and connect an upper gallery back to a known safe ledge. Victory is completing a genuine round-trip traversal loop with base-required strokes, not transporting cargo or touching arbitrary checkpoint rings. The newly opened shortcut remains useful for the entire campaign; existing Swimming/Climbing growth rewards the practiced actions. An egg on an optional exposed extension and a summit keepsake reward further exploration. Optional expert lines use existing feats but the main loop does not.

Three optional challenges: recover a specimen from a shelf with no room for a collector; chain a climb/rest/release/catch sequence; solve a branching vertical route with base strokes. Feats improve execution but never determine whether the main capstone is possible.

### D. Lost-Net Works — roughly hour 3–4

Identity: lost fishing gear snagged among rock in the existing pit region. New grammar: readable attachment/cut order, staged release of loads, capacity planning. No rope simulation required: authored attachment points hold a load until specified strands are cut.

Ten local encounters:
1. **First Loaded Strand:** see which attachment holds a find; cut it over a ready catcher.
2. **Two Supports:** first cut tilts the supported load into its remaining rest position; second releases it.
3. **Cut or Crawl:** a natural bypass is always available; scissors open the efficient extraction path.
4. **Wrong Side:** choose the cut position that leaves you outside the falling/rolling path.
5. **Tangled Bowl:** uncover the bowl before releasing the net above it.
6. **Narrow-Shelf Duel:** an existing shallow-habitat mantis at the upper approach tests guard/parry and retreat discipline. The meal and Fighting progression are the reward; taking salvage is unnecessary.
7. **The Burrow Crossing:** the inhabited upper rim offers a snapping-shrimp combat/avoidance problem with newly arranged solid cover. The win is crossing confidently and learning the attack geometry; no loot container is attached.
8. **Build the Staging Post:** at a safe crevice, select a cache or tool-rest fixture and use it to change a neighboring two-support release problem. The fixture has an immediate function; its material is present in the site. The practice load is part of this encounter, not also counted elsewhere.
9. **Lantern Workshop:** choose a lantern investment or a field-kit investment, then test the choice on a local recess/return problem with both approaches viable. The illumination reveals actual geometry; it does not spawn contents.
10. **The Empty Frame:** a cut net leaves a genuinely traversable route connecting two familiar elevations.

Capstone **Open the Works:** three loaded net sections obstruct different parts of a useful passage. Inspect support attachments, choose a cut order and stage loose loads so the passage remains traversable after release. Victory is physically opening and traversing a continuous route between the two entrances; carrying or displaying a shell is unnecessary. An intact extracted load is an optional bonus, not the objective. The player has built a useful permanent connection by changing the environment. A guaranteed optional culture remains in its authored suitable habitat, not newly invented as a net organism.

What remains after the capstone is the open sorting yard. D8 builds its first guide and D10 opens the second outlet. Two later layouts give manipulation and craft-focused players a reason to reuse that passage after every authored net has been cut. The full operating rules appear under “Returning after the ending.”

Three optional challenges: extract one intact load without releasing its neighbor; catch a two-stage release on a narrow ledge; solve a cut-order configuration from the opposite entrance. Net strands do not regrow on a timer.

### E. Canyon Switchbacks — roughly hour 4–5

Identity: existing flood/ebb, upwelling escape, exposed versus sheltered transport. New grammar: route by flow phase, secure collection against stronger flow, transport with the larger collector. Use actual flow queries; never fake a pressure gate.

Ten local encounters:
1. **Slack Pocket:** discover a sheltered staging basin beside the main current.
2. **Flood Lift:** release into the rising lane and catch at an upper shelf.
3. **Ebb Delivery:** steer a released find into the lower basin while taking the sheltered route yourself.
4. **Glass Below:** recover a sponge skeleton on its established canyon habitat floor; unlock existing grips.
5. **Bare-Rock Return:** grips open a useful return and optional side chamber.
6. **Anchored Bowl:** add the collector brace before an exposed extraction, or choose the sheltered slower route.
7. **Rim Hunter:** on the established shallow upper rim, an existing moray lane provides a standalone fight/escape challenge. It remains outside the deep fauna zones. Fighting experience, a possible existing body meal and a reliable crossing give reasons to engage.
8. **Rim Sparring Ground:** a distinct shallow-rim shrimp confrontation emphasizes a short punish window and retreat. It supports deliberate combat practice; no collectible guards the exit. Heavy/light object behavior instead appears within encounter 3's single counted transport problem.
9. **Buried Landing:** excavate a catch recess on a shelf before the release above.
10. **Across the Middle:** crossing and object transport use different paths; plan both.

Capstone **The Canyon Lift:** recover a large hollow shell from a lower shelf, use water and the collecting shell to move it across two elevations, then return through your chosen shortcut. An error drops it into a reachable lower bowl, never into an infinite void. Reward: distinctive specimen and access toward F. Existing always-upwelling escape remains reliable.

What remains after the capstone is a drift fishery for ordinary empty shell material, plus the inhabited shallow rim. E1 and E3 teach two viable intercept positions while completing their existing authored challenges. The incoming shapes and actual current determine the next catch. The prepared-food and combat options serve a player who arrives to fight instead.

Three optional challenges: transport a light/heavy pair to separate shelves; extract on the exposed route with no collector brace; raid the existing octopus den on the shallow far rim using its existing arm/parry/ink fight and an extraction retreat. Keep that animal in its established habitat, not on the deep canyon floor. The octopus is a substantial optional combat encounter (allow 10–15 minutes rather than six); the content budget conservatively does not add those extra minutes. Do not make standing idle for flood part of the expected time budget.

### F. Living-Lights Basin — roughly hour 5–6, optional returns into hour 7

Identity: the existing living sea-pen lights and the visible consequences of a finished expedition. New grammar: inspect in darkness with existing lantern, combine long-route planning with physical extraction, trace a real biological reveal. No new deep predator without sourcing and a separate creature implementation.

Ten local encounters:
1. **First Brush:** the existing sea-pen response reveals that the lights are living.
2. **Light in a Crack:** a better lantern reveals the actual approach to a side pocket.
3. **Dark Return:** visible remembered landmarks let the player plan a short route without HUD arrows.
4. **Buried Bowl:** excavate a large specimen without burying its exit in spoil.
5. **Long Release:** stage two collection positions before releasing from a higher pocket.
6. **Outfit for the Upper Window:** use the local safe tool-rest to choose a transport-oriented collector or a traversal/food loadout, and solve an adjacent upper-window route with that choice. Practical comparison and execution constitute this encounter; menu time does not.
7. **The Deep Pantry:** establish a bounded prepared-food cache at a crevice and test a recovery route using it. Choose immediate health recovery versus reserve-focused preparation. The cache changes which excursions are comfortable; never require stocking a quota.
8. **Last Loaded Net:** apply cut-order and flow knowledge together in a compact authored setup.
9. **A Place to Return To:** finish a chosen refuge fixture and make a short real excursion that uses it. Take a stable record of this outpost into the home view; functional shelter identity is the reward, not another display shell.
10. **The Sheltered Specimen:** a compact optional-within-district extraction that combines support, a buried outlet and a short catch trajectory. It contributes physical salvage variety while the final capstone handles the large-scale discovery.

Capstone **The Lights Were Alive:** reach three connected viewpoints through the meadow: the first close interaction shows the existing sea-pen response, an upper approach reveals the extent of the living meadow, and the far return makes the earlier distant lights geographically intelligible. Each viewpoint requires an actual route choice using light, current, climbing or excavation; there are no collectible observation tokens. Victory is witnessing the living-light reveal and completing the open return circuit to the refuge. The ending card follows there, with optional return home and Keep exploring. A difficult large specimen remains available as an optional extraction within the already-counted side challenges. Move the current immediate floor-arrival ending trigger to this completed exploration circuit. No cargo delivery, photo quota or mandatory combat boss.

What remains after the capstone is a playable meadow. F1 teaches brushing the existing illuminated patches, F6 gives a choice of marking positions and F9 installs a refuge sketch board for recording and displaying a route. Subsequent visits support composing a new light trace, scouting another approach, or answering a partner’s trace. The markers, recorder, trail display and co-op transfer require implementation.

Three optional challenges: recover a second specimen through the narrow return; solve a combined two-support net/current extraction; trace an alternate upper approach while preserving a lower route for a friend. These are concrete spaces to author, not medals layered onto the final sequence.

## Parallel progression and reward economy

Combat, movement and excavation remain independent ways to make progress. New skills and XP are permitted when they recognize useful new actions; they cannot substitute for building those actions or supply missing hours. Existing Fighting/Swimming/Climbing grow through their actual uses. Main routes can use base controls plus found/crafted equipment; feats provide additional options on return visits.

Proposed tool/recipe contract:

| Tool | Inputs | What changes immediately | First useful context |
|---|---|---|---|
| Collecting valve I | 2 valves + 1 grit | one deployable catcher, capacity 2 small finds or 1 medium | A, choose placement before release |
| Shell scoop | 2 valves | toggle narrow default sweep / wide shallow sweep; precision vs breadth | A/B spoil management |
| Clam guards | existing 2 valves | existing block protection | B direct raid route |
| Scissor claws | existing 1 tooth | existing nets plus authored load-support strands | B/C tooth into D cut-order problems |
| Collecting shell II | 1 ordinary intact bowl shell + 2 valves | replaceable collector body: capacity 3 small or 1 large; larger footprint | C/D staged and large loads |
| Collector brace | 2 grit + 1 valve | supported collector resists stronger flow; consumes no fuel | D/E exposed staging |
| Glass grips | existing 1 glass sponge | existing bare-rock climbing | E return/side routes |
| Brighter lantern | existing 1 culture | existing increased illumination radius | D/F inspect real recesses |
| Home growth | existing grit thresholds 1/3/5 | existing comfort/storage plus visible display integration | optional throughout |

Large-shell collector replaces the previous equipped collector, not another hotbar item. Both profiles remain available by swapping at home. The brace is a permanent fitting. The initial shared loose valve lets the player understand catching before crafting. No repair tax or consumable tool-charge meter. Optional seeded rare appearances and sidegrade finds are allowed; required recipes/materials have guaranteed authored sources and never depend on random drops.

Material supply is designed around choices, not scarcity grind. Each district provides several accessible clams/common shells through its encounters; completing any two of A's branches should supply a first tool. B and C each provide a guaranteed tooth opportunity, owned per player under the existing progression-item rule. E supplies the canonical sponge. Optional culture placements need habitat review. Two parallel routes must not consume the same unique material irreversibly: tool-critical shells have ordinary equivalents, and displayed specimens can be retrieved.

Food preparation consumes renewable clams and yields ordinary valves. The D and E inlets can introduce ordinary loose shell material from outside the active play space. An intact common valve can become one valve of crafting stock. Crushing one common chip yields one grit. The separate authored shell retains its specified two-grit outcome. Prepared food supports an outing. Reusable tools and outpost fixtures support different working arrangements. At full equipment, spare stock can go into an optional home inlay or stay in circulation. Each home has one reversible 12-cell panel with three ordinary inlay shapes costing one grit or valve per newly filled cell. Rearranging a filled cell is free. This crafting/display mechanism is new. It offers an occasional use for stock and an expressive goal for interested players.

The plate can fill, food can be plentiful, and storage can reach capacity. In that state the player can leave a catch in its basin, release it through a visible downstream outlet, spar on the rim, operate the yard for the pleasure of sorting, or compose a meadow trace. The playtest must cover precisely that state. Players who enjoy a familiar harvesting or fighting rhythm can keep doing it. Required advancement and release-duration estimates use the authored campaign.

One-time finds, cut nets, opened shortcuts, remembered landmarks and earned capabilities persist. Stage common material through bounded source slots at recognizable inlets. Existing animal return and clam-bed regrowth continue under their own rules. Moment-to-moment sediment movement follows the actual simulation. All changes to a player's excavation come from visible physical processes. The renewable systems are required late-game features with their own pre-release gate.

### Parallel activities: reasons to play that do not depend on salvage

The campaign must support the player saying **“Tonight I want to get better at fighting,” “I want to build a useful forward camp,” or “I want to outfit for that climb”**, not only “I want the next shell.” Excavation is a signature verb, not the purpose of every system.

| Self-directed activity | Repeated active decisions | Useful reward path | Independent success |
|---|---|---|---|
| Combat practice and hunting | choose a confrontation; read tell; counter, block or disengage; decide whether to spend carried food | existing Fighting levels/feat; existing defeated-body healing/reserve; safe confidence in crossings; one-time visible combat mementos | beat a specific encounter or execute a chosen clean response; no cargo required |
| Prospecting and salvage | inspect, expose, preserve/process, release and recover | Salvaging mastery, collector/scoop methods, ordinary materials, optional rare appearances | recover or process the intended object in a way the player chose |
| Food preparation and expedition outfitting | choose health food or reserve food; choose what occupies limited carry/cache slots; use it on an outing | field recovery and longer comfortable routes; valves for equipment | complete an excursion with the chosen loadout; no meal-delivery objective |
| Field crafting and refuge building | choose a fixture for a real crevice; install it; choose equipment/storage use; test it in adjacent terrain | a cache, a tool-changing rest, a recognizable refuge network | the fixture functions and changes the player's next action |
| Movement mastery and route making | choose climbing/current/swimming line; conserve strokes; excavate useful landing/return | existing Swimming/Climbing levels/feats and eggs; permanent useful connections | complete a route or reach a viewpoint; no object transport required |
| Collection and home identity | keep a striking find, pursue a variant, display combat/exploration mementos, choose visible fixtures | a home reflecting the player's interests, optional collection satisfaction | achieve the player's own display/build goal; no completion bonus required |

Combat must remain worthwhile when there is no salvage nearby. Ten local encounters are explicitly combat/ecology-focused, and the existing optional octopus encounter remains a substantial combat goal. Replays of combat encounters can support practice and existing XP/body meals, but repeated kills contribute **zero** to the authored five-hour content budget. Do not force animal fights on explorers or require particular enemy drops for essential tools. Combat mementos are nonphysical records of an accomplished encounter presented as home markings, not invented body parts awarded out of nowhere.

#### Food: a small real preparation loop

Add two preparation recipes after the first slice, using the existing clam/larder/valve economy:
- **Clam portions:** one stored clam → two prepared portions. Each restores 20% of maximum shell when consumed out in the field.
- **Reserve portions:** one stored clam → two portions that each restore three strokes' worth of reserve, capped at the crab's existing reserve maximum. They do not add stroke capacity or grant height directly.

Both recipes leave the clam's two valves exactly once. A clam can become a normal existing meal OR prepared portions, never both. Retain the existing ordinary well-fed meal and its numbers. Expose the auto-eat preference in the home menu so deliberate preparation is not preempted by automatic eating; existing default behavior can remain selected until the player chooses preparation.

Carry at most two portions, each occupying a small carry slot. Consumption requires a 2-second stationary action, cancels on damage/movement without consuming, and is unavailable while soft or dead. This is recovery between encounters, not healing while holding block. Eating at a safe crevice is unnecessary because existing rest already heals; the value is recovering on an active outing away from a refuge. Prepared portions persist and do not spoil on a real-time timer. These are tuned game-food effects, not biological claims.

This supports an independent foraging → preparation → adventurous route/fight loop. Food makes choices more forgiving; none of the authored routes require carrying it. Preparing portions at a menu is not four minutes of content: the counted encounter includes a distinct nearby outing where the choice is used.

#### Building: six useful outposts, not a new construction engine

Author at most one fixture socket in each district's designated safe crevice. A player may install one of two fixtures there:
- **Field cache:** 2 grit + 2 valves; four shared slots for ordinary materials/prepared portions. No automatic delivery, no teleportation and no progression-item monopoly. An empty cache is still usable without filling a quota.
- **Tool rest:** 2 grit + 2 valves; allows changing between owned collector bodies while tucked safely into this crevice. The spare bodies are visibly racked there when owned; changing the active narrow/wide dig sweep remains available during ordinary play and is not artificially restricted to this fixture. It is not a remote crafting shop and creates no free materials.

Swapping returns the old fixture's material to inventory/cache if there is room, otherwise leaves a recoverable bundle; no demolition tax. Fixtures use fixed authored sockets and do not alter terrain collision. Existing safe-crevice healing remains unchanged. Fixture choice is shared for that world and should be visibly named before confirming a co-op replacement. No player may lose stored contents on replacement/disconnect.

At home, show the selected refuge fixtures as a small physical route record and allow two display arrangements: workshop emphasis (tools/valves prominently displayed) or trophy emphasis (specimens/mementos prominently displayed). This is authored prop arrangement, not a free-placement building editor. Existing home-grit growth stays intact; a player can pursue home comfort, an outpost network or collection identity without being ordered to complete all three.

#### Skilling and useful equipment tiers

Keep all existing Fighting/Swimming/Climbing XP and feat thresholds. Their standalone encounters now give actual reasons to practice. A new **Salvaging** skill is appropriate for the complete campaign, **after** the physical slice proves itself; it is explicitly not necessary for the first slice.

Proposed finite mastery awards per stable find: first meaningful exposure 2 XP; intact recovery 3 XP OR intentional processing 3 XP; successful catcher interception 2 XP. Each award bit is recorded once, so repeated bury/release and reload farming give no XP. First completion of a unique authored net-support configuration gives 4 XP. XP is awarded for outcomes, not seconds holding Dig. Exact thresholds are proposed and must be checked against normal encounter logs.

| Salvaging rank | Total XP | Practical milestone |
|---|---:|---|
| 1 | 0 | default digging, initial loose catcher and collecting valve I |
| 2 | 14 | shell scoop recipe; precision versus wide clearing |
| 3 | 42 | collector brace recipe; exposed versus sheltered placement |
| 4 | 84 | collecting shell II recipe; greater capacity with a larger footprint |
| 5 | 140 | visible home mastery marking and optional expert extraction map-memory annotations for sites already seen; no mandatory power gate |

This is learn-by-doing progression through different jobs, not a reason to repeat a depleted bank. Two successful early multi-find encounters should make rank 2 possible. Rank 4 should arrive through ordinary A/B/C participation before it is attractive in D/E. If observed logs miss that, adjust this *new* skill's award economy or supply an alternate technique—not existing feat timing and not an order to farm. Basic methods remain viable in required passages; the larger-collector capstone has a supported manual/sheltered alternative for an under-skilled explorer. Field fixtures and food recipes are available by default, independent of this rank.

Optional rarities are welcome: seed visual variants at initial site creation, persist the result, and mix a few authored uncommon finds into distinct habitats. A rare item may offer a different collector appearance or a clearly described sidegrade profile, never the only scissors/grips/required collector. Do not count time spent rerolling or hunting a rare drop toward the release duration. Required recipe inputs remain guaranteed.

### Revised content accounting: same counts, different purposes

The 60 local encounters are now allocated by **primary purpose**, with no activity double-counted:

| District | Salvage/physics | Combat/ecology | Traversal | Craft/prepare/build + field test | Discovery | Total |
|---|---:|---:|---:|---:|---:|---:|
| A | 6 | 0 | 1 | 2 | 1 | 10 |
| B | 3 | 6 | 0 | 1 | 0 | 10 |
| C | 3 | 0 | 5 | 1 | 1 | 10 |
| D | 5 | 2 | 0 | 2 | 1 | 10 |
| E | 3 | 2 | 3 | 1 | 1 | 10 |
| F | 4 | 0 | 1 | 3 | 2 | 10 |
| **Total** | **24** | **10** | **10** | **10** | **6** | **60** |

At the existing provisional four-minute mean, these supply 96 + 40 + 40 + 40 + 24 = **240 minutes**, unchanged. Craft/build time counts active testing in its own authored setup, not menus or hauling ingredients. Animal encounters in E are on the established shallow upper rim, never new creatures in the deep basin.

The six capstones now comprise **two extraction/transport victories (A/E), one court/route victory with combat alternatives (B), one traversal circuit (C), one construction-by-clearing victory (D), and one discovery/return circuit (F)**. They still total a provisional 72 minutes. The 18 optional challenges and their 108-minute conservative allowance remain unchanged. A typical selection still totals about 312 engaged minutes; available content remains about 420. These are planning estimates, and the actual variety/time must be demonstrated in play.

Manifest assignment (numbers refer to the named encounters above): A salvage 1/2/3/5/7/9, traversal 8, craft 4/10, discovery 6; B salvage 5/7/10, combat 1/2/3/4/6/8, craft 9; C salvage 1/5/6, traversal 2/3/4/8/10, craft 9, discovery 7; D salvage 1–5, combat 6/7, craft 8/9, discovery 10; E salvage 2/3/4, combat 7/8, traversal 5/9/10, craft 6, discovery 1; F salvage 4/5/8/10, traversal 3, craft 6/7/9, discovery 1/2.

The primary-purpose table is a production constraint: each district's encounter manifest must identify which named entry occupies which category. A fight plus a shell in the same room is one encounter, not two. If a supposedly craft-focused room is merely another excavation with a recipe toast, it belongs in salvage and the missing craft activity must be redesigned within the existing slot.


## Returning after the ending

A returning crab can leave the refuge with a full pantry, every tool and every fixed trophy. From the opened Works it can hear moving stock at the inlet, from the canyon lip it can see the direction of the water and which rim crossing is occupied, and in the meadow it can see the light traces it and its friend last composed. These are three reasons to start playing again. Keep their entry points close to a safe crevice and show their conditions in the scene. A trip made 30 minutes later might start with the same desired activity and a different physical decision.

**D. Work the shell yard.** Once the player opens the two permanent net passages, one becomes a sheltered receiving bowl and the other an exposed lower run. At the inlet a broad valve is sliding ahead of a compact chip. The crab can tip a reusable guide at one of two supported sockets, excavate a receiving recess and release the pieces in the order it chooses. Route the broad valve intact for preparation stock or to use as a physical collecting piece. Send the chip to the breaking stone and deliberately process it for grit, or send it back out with the current. See/hear each separate impact and watch which outlet it actually reaches. The sorting work starts and finishes in the yard.

Next batch, the chip arrives first from the lower mouth. A broad guide that delivered both earlier pieces to the quiet bowl now blocks the lower rolling line. Recover the chip from the accessible spill bed, leave that guide for the next broad valve and hand-intercept the small piece, or shift the guide and commit the catcher to the other outlet. A player wanting grit can deliberately choose a different output from a player wanting intact stock. A player content to sort at an optimized station can repeat the familiar cut. The yard stays laid out as its owner left it. Advance beyond prototype only if two physical input mixes make different guide/catcher positions useful and a deliberate input-order change produces an observable outcome. Provide one reusable ordinary practice piece on a reachable ledge for immediate play even if every commodity slot is occupied elsewhere. Both players can operate a guide and catch independently. Solo can stage the guide before releasing stock.

**E. Run the canyon drift.** A flat empty valve appears at the upper lip and current streaks show upward motion. Stage a catcher on the upper shelf and intercept early, or use the sheltered eddy and reach for it by claw. A compact, faster-sinking piece at the lower notch makes the lower catch attractive. A missed interception changes the next decision immediately. Cut across the moving lane, move the catcher to the downstream bowl, or let it pass while going to the nearby rim. A catcher filling at the first opportunity leaves the next piece available for hand interception. Contents are visible. A missed piece has a reachable terminal basin. Choosing to release it through the outflow retires its ordinary material claim. Grips and swim techniques change the available approach, and the later broad/compact collector choice affects what fits where. Reaching all gear increases the choices a player can make here. A solo crab can take each route with the basic catcher or by hand. A pair can split the two shelves and swap jobs.

The initial E build contains **one anchor, two inlet coordinates, two renewable ordinary slots total, one resident reusable practice piece, one repositionable basic valve and two distinct ordinary body profiles**. Prototype different placements before requiring the future collector upgrades. In the inspected implementation, salvage integration samples horizontal CA flow and downward gravity. Vertical tide/upwelling coupling is new work. Sample local cellular water and the existing world tide/upwelling query at sim tick on the regional owner. Verify the resulting trajectories on actual canyon geometry, both flow phases and two material profiles, with movement/recovery paths reachable by the crab. In the codebase these are distinct paths through cellular water and `Level::water_at`. Calibrate axes/units and avoid applying the same current twice. Use a slope-and-eddy fallback during development if the test fails. Only ship the flood-lift promise after both-axis transport works on the target devices.

**E. Hunt or cross the shallow rim.** On a return from the canyon, see whether the existing moray is at its crevice, pursuing existing shrimp or engaged with the crab. Take the protected route, wait out a single telegraphed lunge by actively dodging/countering, or approach a separate inhabited ledge for a mantis fight. A fight at full skill still gives a chosen test of execution, a body meal where existing rules allow, and a newly safe window to cross. The next approach can find the same animal holding another position or a shrimp already targeted by the eel. Use actual actor state, visible headings and existing attack tells. A player arriving while the lane is quiet can enter the drift run or meadow immediately. Verify repeatable actor occupancy and retreat behavior on the current build. Keep ordinary animal return rules and the octopus accomplishment intact. A partner may guard an exposed landing while the other intercepts drift, but each pursuit works alone.

Combat renewal has a specific prerequisite. The inspected generic enemy step constructs a replacement animal at its authored home when the Gone countdown expires, preserving the death counter. The octopus has its own den-vacancy condition. These are live-sim rules. The inspected save/progress definitions do not establish durable whole-population restoration. For the two repeatable rim habitats, add or prove a bounded per-habitat record containing occupant generation, actor checkpoint or vacant state, remaining active-sim return interval, prior body-meal claim and current regional authority. After a defeated animal is consumed, preserve the vacant habitat and consumed receipt across page eviction, save/reload and rejoin. At the legitimate existing return threshold a new generation enters from a visible crevice/edge while it is clear, with a readable tell before engagement. Keep one occupant per habitat. Arrival presentation, occupancy checks and durable lifecycle support are new requirements unless demonstrated on the integrated build. One-time fighting accomplishments stay earned. If this cannot be demonstrated, E's renewable combat lane remains unfinished and cannot earn credit in depleted-world playtests. The yard, drift and meadow remain available during vacancy.

**F. Play the meadow.** The first discovery explains the original blue lights. A repeat visit lets the player make something with them. From the refuge, pick up to three reusable pale shell markers and place them in any of six authored safe sockets at different heights around existing reactive patches. Choose Trace on the board. Swim, drop, climb or brush along an actual path. Capture a bounded pattern of the resulting patch contacts and positions, including pauses where the player intended a break. Back at the board, view the line and its rhythm as a short animated sketch, keep a favorite in either of two slots, or make another immediately. An unrecorded run still gives the same visible living-light response.

Example: the player makes a continuous low sweep across three patches and a long gap before a high arc. Next time the flood makes the outer ascent attractive. Move one marker near the high wall and draw a branching answer that joins the old arc at its endpoint. Compare the two traces at the refuge. A solo crab can loop, mirror or extend its prior trail in pieces. Two crabs can make intersecting strokes together or take turns answering asynchronously within the saved local world. Discovery after the map is known comes from finding another workable line and seeing its shape. Keep the actual cut channels, markers and last traces, so returning players recognize their own handwriting. Capture by actual contact only, and render the recalled trace on the board or in crab memory. The animals themselves have only their established response. The route recorder, movable markers, sketch comparison, rendering and bounded co-op transfer are new mechanics. Prototype this as an expressive plaything. If players find placing markers dull and never keep or answer a trace, rework that interaction before counting F as a return destination.

### Readable renewal and bounded state

- Give D and E each a fixed pair of visible inlet positions, two renewable commodity slots and reachable settling/outflow geometry. Show material entering across the inlet boundary with ordinary motion and sound. A temporarily blocked inlet defers admission to the other clear mouth or next eligible event. A full receiver allows a readable spill to the overflow path. A blocked region cannot silently delete a trapped object to make room.
- The active region owner admits at most one fresh slot on a suitable simulated flow transition. In D use a measured flow-direction or velocity-band crossing with hysteresis and a minimum separation. In E use a tide/flow phase crossing measured in the active canyon. Persist the last consumed crossing token and the upcoming shape/order from a bounded seeded deck. Expose the leading shape before commitment. Permit comfortable repeat catches. Never make a rare profile necessary. A fixed practice body in each yard is available on demand to place/release by claw and cannot convert into loot. F is playable at any phase. No calendar clock or offscreen catch-up generates material.
- Keep `(anchor, slot, generation, revision)` as a distinct renewable identity. Persist the phase token, bounded deck cursor, material kind, pose/velocity, owner, and any pending receipt/transfer. One generation can be loose, caught, settled, carried, processed or deliberately washed out. Recycling requires durable terminal state and resolved receipts for that generation. Delayed old messages cannot grant newly reused slots. Convert ordinary commodity carry to bounded fungible inventory on acknowledged collection. Durable unique find IDs never enter this pool. Persist only fixed records and two saved sketches of at most 24 quantized contact events each per board. Hold a single fixed working sketch per local player. Active dynamics require explicit body caps and dormant-region behavior. Page fixtures initialize once and reload from their durable state. Disconnect, region handoff and reopening the page do not advance the inlet sequence.
- The encounter engineer reports physical site IDs 1–64 and up to four spatial component records per authored site in the work in progress. Activities and site records have different cardinalities. Assign the four renewable slots their own bounded schema and clearly separate namespaces. Expanding a documented bound requires a measured cost and an explicit format replacement under the project’s no-migrations policy. Current finite component records alone do not implement renewable actors. Existing local saves, regional owners and co-op channels suffice as infrastructure. No online service or shared global calendar is required.
- The water, sediment and hunting states can change while a player is present. Its channels, fixed treasure outcomes, guide/fixture positions and saved traces remain recognizable. Use deterministic sim time during active play. Cap upcoming arrivals while unloaded. Existing meal use, optional inlays and voluntary fixture configurations supply material demand. Practice, combat, exploration and expression remain available at full storage, maximum rank or zero material reward.

### Return-play release checks

1. Save after resolving every fixed find, unlocking all tools/feats, cutting every authored net, opening shortcuts, completing the ending, filling the pantry and taking every display specimen. Defeat all one-time encounters, defeat and consume the rim occupants, and save mid-vacancy. Prove that reload neither reissues the meal nor silently fills an empty habitat, then observe a legitimate next-generation arrival through active sim play with its body claim available once. Reload solo and in co-op. Immediately play D, E and F with no new one-time claims. Check another load after two activities and a region handoff. One pass must leave the home/outpost inventories full.
2. Replay the same optimized yard against two distinct physical input mixes. Verify a different intervention materially changes the recovered output, the actual contact path, or both. Also permit a familiar reliable method for players who enjoy routine. Verify a missed catch produces a meaningful reachable next decision and a flood/ebb change visibly alters representative E trajectories on native and wasm. Save and return to the exact excavated channel and guide arrangement.
3. Exercise blocked inlets, packed catcher, both slots occupied, actor absence, depleted beds, disconnected catcher owner, reload during pending grant, full journal, stale previous-generation delivery and source/destination transfer interruption. No unique reward returns. State stays bounded across 10,000 simulated flow events. The practice and meadow pursuits stay playable even when no new stock can enter.
4. Compare two actual meadow paths at phone size. Distinct contact order and pauses produce distinguishable traces. A second player can recognize where to join a trail, answer it, save both traces, reload, and keep exploring solo after disconnect. Contact never triggers through solid terrain. Verify enemy tells remain readable amid silt and falling material.
5. Observe at least five returning players with the completed save and no to-do list, including three solo sessions and one pair. After orientation, give them 20–30 minutes with freedom to stop or leave the late districts. Record which pursuit they choose, each voluntary replay, what they changed or deliberately repeated, and why. Repeat a short visit later from their actual saved situation. Include a pass where all material rewards are declined. Design target: at least three players choose another bout/run/composition without prompting, two distinct pursuits attract voluntary returns across the group, and players can identify either a satisfying practiced action or a changed decision. Report each observation and the distribution. This small sample is a decision aid.
6. Existing opening recordings were reported to show about 20–30 seconds between release and catch, much of it spent watching. Separate active positioning, meaningful anticipation, and involuntary idle transit in late playtests. In a cycle intended for interception, aim to offer an actionable change within 3 seconds of release and a first intercept in about 4–8 seconds. A longer trajectory must offer reachable midcourse choices. Flag any forced idle segment over 8 seconds and any run spending over 25% of engaged time simply waiting for arrival. Voluntary pauses to watch the water are fine. Once layouts are optimized, remeasure while leaving the player's best setup intact. Compress empty spans or add usable intercept positions if the player can set down the controls and get the same outcome.
7. Keep the authored 5–7 hour estimate and depleted-world review as separate release criteria. Do not multiply a short pleasurable loop by an assumed replay count to fill duration. If players exhaust F and voluntarily move everything back to the opening, improve the late pursuit and its rewards before adding more opening content.


## Meaningful repeat variation

Variation comes from different support arrangements, sediment type, slopes, flow direction/strength, outlet choices, collector capacities, creature placement and the player's previous excavation. Change at least two of these when authoring a remix, and change the best plan. A recolored shell in an identical hole does not count.

Player skill expression includes leaving a support until ready, approaching an exposed face cleanly, staging an interception, timing an existing dodge/parry, conserving swim strokes, cutting supports in a useful order and leaving a safe return. Players can also choose the slower robust method. There is no requirement to master every combat or movement technique to see the ending.

Three load-support templates suffice for D initially: single restraint, two independent supports with an authored intermediate resting pose, and two nearby independent loads. Do not implement general swinging ropes, deformable nets or arbitrary joints. Each valid authored pose must agree with visible attachment positions and terrain.

## Controls, readability and accessibility

Keep all movement/combat/dig controls. Add one contextual interaction for take/set/retrieve/process. Keyboard/controller binding must be selected against the actual current input map; do not commandeer an existing action. On phone, one large context button appears only in reach, with a noun/action label. Hold or a small explicit choice panel distinguishes destructive processing; no precision dragging or tiny inventory grid.

Prioritize crab and enemy tells over opaque silt. Sediment can obscure a find, but cannot hide a lethal tell completely. A released object makes a distinct sound and small local motion cue, not a full-screen reward overlay. Show collector contents and capacity physically plus a compact count while nearby. Distinguish settled, snagged and free objects through motion/attachments, not color alone.

First-use hints disappear after that action and remain retrievable in settings/help. No long tutorial modal. The map is crab memory: revealed routes and locally discovered opportunities only, no omniscient treasure icons or GPS objective trail.

## Solo, co-op and recovery promises

Everything can be solved alone. Two players can divide release/catch roles, guard/excavate, or explore alternative routes. No simultaneous switches, mandatory player boosting, voice requirement, loneliness penalty or multiplayer-only reward.

Terrain, ordinary finds, collectors and cut attachments are shared. Ordinary resources are atomically taken once. Progression rewards preserve the existing per-player entitlement rule: for example, a shared excavated cache exposes a personal tooth claim for each player rather than spawning duplicate physical teeth into the same moving body. Cosmetic ownership can be one physical specimen, but both players can independently complete their own collection in their own worlds; do not gate gear behind the friend's trophy.

Persistent gear and skills survive death as now. Carried ordinary finds join existing cast-off recovery. Released finds remain at their actual settled position. Every site's authored bounds include a reachable overflow basin; reject a layout where required finds can leave the playable region. If a body becomes invalid because of an engine fault, return it deterministically to its last validated safe basin and emit a diagnostic—not an exploitable player teleport feature.

A full collector or full shell leaves the find visible and available. Retrieve a collector with contents only if carrying capacity permits, otherwise offer empty/leave. Disconnecting cannot delete shared finds or duplicate a claimed material. Guests do not mutate another crab's inventory. Reloads preserve support/cut/object state and cannot reroll rewards.

## Implementation contract and verified reuse

Read-only inspection covered CLAUDE.md; vision/how-it-plays/world/ocean-shaped/universe; craft.rs, level.rs, physics.rs, shared.rs, relevant world.rs/enemy.rs/underwater.rs sections, and the prior demo backlog. The companion patch updates the campaign specification only.

Existing reusable hooks:
- `underwater::World::cell`, `flow_at`, world/local cell transforms, `game_dig_support`/`game_dig_load`; actual sediment displacement and creature wakes.
- `Level` authored geometry/climbs/nets/brittle/current regions, `Level::solids`, safe crevices and existing world navigation.
- Existing directional Dig, body movement, carry slots, food, death recovery, workbench recipes and per-player equipment.
- Existing `enemy` states/tells, animal-prey hunting and physical line/path checks; do not rewrite creature AI for content.
- Existing shared take arbitration, per-player progression claims, deterministic replay/save and regional CA authority.

Implementation status at integrated revision e03432c:
- PR #355 already contains an opening Shellwash body, valve positions, fragment claim records, prepared portions and the shallow shortcut. The integrated spec and the companion multi-site checkout represent different work in progress. Verify their merge before building new features.
- Ordinary fixed pickups still need integration when converted into physical sites. The opening find demonstrates basic support and transport. Mixed profiles, reusable guides, renewable inlet generations and full vertical canyon transport still require work.
- Load-support attachments/cut ordering, scoop sweep profile and collector fittings are new behavior.
- Content authoring, phone affordances, home trophy presentation and a later ending trigger are substantial deliverables. The broader campaign also adds explicit meal preparation, bounded field caches, equipment profiles and activity mastery; these are separately budgeted below, not claimed as existing hooks.
- Terrain affecting an attack must be checked per attack type. A safe-looking tunnel that a snapping wave/bite crosses is a bug, not difficulty.

Suggested module boundary: `crates/sim/src/salvage.rs`, `salvage/content.rs` and existing presentation/input modules. Names are recommendations; contracts are mandatory. Keep all rules in deterministic Rust simulation. No JS/Python gameplay implementation or new physics dependency.

Suggested serializable records:

```text
SiteDef { id, district, bounds, hints[], finds[], outlets[], safe_basins[], attachment_defs[] }
FindDef { id, kind, shape, appearance, initial_pose, support_samples[], exposure_samples[], entitlement }
FindState { id, phase, pose, velocity, last_safe_basin, support_stability_ticks, owner_claim }
FindPhase = Embedded | Exposed | Free | Settled | Caught(collector_id) | Carried(player_id) | Consumed
CollectorState { id, owner, profile, brace, pose, phase, contents[] }
CollectorPhase = Packed | Placed | Free | Carried
AttachmentState { id, intact, supported_find_ids[], remaining_pose_index }
SalvageCommand = Interact(target_id, action) | PlaceCollector(pose) | SetDigProfile(profile)
SalvageEvent = Exposed | Released | Caught | Missed | Claimed | Processed | AttachmentCut | Displayed
```

Separate durable states from derived visual exposure. No random loot on repeated Dig. Stable IDs survive save, regional transfer and replay. Authored variants and optional initial cosmetic-rarity selection consume seeded RNG and persist the result; never use render-frame randomness or reroll on reload.

Transition rules:
1. Embedded ↔ exposed depends on real local coverage; visibility may recede if sediment covers it again.
2. Embedded/exposed → free when support is genuinely removed or the last attachment is cut; emit release once.
3. Free → settled on valid resting contact; settled → free when support moves.
4. Free/settled → caught only through valid mouth intersection and capacity/size check.
5. Exposed/settled/caught → carried only after reach, exposure, path, capacity and authoritative ownership validation. Embedded pickups cannot pass through terrain.
6. Explicit processing consumes one eligible empty shell and creates exactly two grit claims/bodies with stable child IDs. Never both intact and grit.
7. Carried → stash/display/tool/death recovery follows existing local-crab inventory authority with the world claim committed exactly once.

Step order must be defined and replayed: apply accepted commands; existing terrain/creature physics; update find supports and attachments; integrate free bodies with swept collision; resolve catcher contacts; arbitrate claims; emit events. Adapt to existing sim order while preserving deterministic causal order and a single owner for each state.

The host/session authority handles shared find/collector claims and metadata; regional CA owner supplies authoritative support/contact observations. Do not independently integrate a shared body on both clients. Transfer ownership at a region boundary through the existing ownership mechanism; if no suitable object handoff exists, explicitly implement it before spreading sites across regions. A per-find monotonic revision and accepted-claim ID should prevent stale updates from resurrecting items. Guest interaction remains responsive through a reversible local anticipation animation, never speculative inventory duplication.

Use the integrated storage branch’s bounded page caches and atomic record path. Validate the relevant record and regional authority interfaces against the implementation checkout when scheduling each feature. Save authored IDs and durable object state through that record path. Avoid scanning a whole world: query support/exposure only in bounded active-site footprints, and sleep settled bodies until terrain changes or nearby activity warrants a recheck. Proposed cap: 24 awake free find bodies per active district, 8 per encounter. Measure actual device cost; these caps are initial limits, not performance proof.

### Bounded persistence and ownership detail

The new props cannot be serialized as an ever-growing item list in the global save head. Store fixed-capacity `SalvageRegionRecord { schema, region_id, revision, site_states[], find_states[], attachments[], collectors[] }` records beside the existing spatial records. The small head contains only the active save epoch/reference and necessary bounded session metadata. Immutable site definitions remain in shipped content.

Proposed limits: at most 8 authored finds per local encounter (up to 12 in a capstone, only 8 simultaneously active); 24 awake free bodies per district; two placed personal collectors plus the bounded authored field valves in a two-player session. Dormant sites keep compact records, not active physics bodies. Do not add an unbounded event history. Benchmark limits before finalizing.

`FindId = (campaign_content_version, site_id, slot_id)`; processed fragments use reserved child slots `(parent_id, child_index=0|1)`. An object uses one representation at a time: regional body, collector contents reference, carried inventory reference, displayed reference, or consumed tombstone. Its fixed record stores the current representation and revision. Crushing never creates arbitrary new IDs. Each authored slot has at most one final consumed/claimed tombstone; record count is bounded by shipped content. Renewable D/E material uses a separate bounded pool and generation counter described in the return-system contract. The claim key includes its generation. Fixed authored slots and their tombstones retain their original identity.

Regional unload writes pose, phase, attachment state, contents and ownership revision atomically before evicting. Load restores those exact bodies; it does not respawn from initial layout. Crossing a region boundary transfers a stable ID and its single authoritative state through a revisioned handoff; source keeps a transfer marker until destination durability/authority is acknowledged. While unacknowledged, source or destination may show an anticipated body but only the designated owner simulates/can grant it. Do not run duplicate forces on both sides. Ownership transfer must be tested while a find is inside a moving/transported collector, not only for loose bodies.

Inventory grant is a durable claim transaction, not a network toast. Conceptual states: Available → GrantPending(claim_id, recipient) → GrantFinalized. The recipient applies the receipt once and persists inventory plus its receipt before acknowledging; world finalization is persisted before allowing the receipt to be retired. An interrupted pending grant is resent under the same ID and never releases the physical find for a second claimant. Keep a bounded pending-receipt set (proposed 32 per player); refuse additional claims gracefully if it is full. Do not silently discard an unresolved receipt. Reuse the project's existing claim/atomic-save protocol where it provides these guarantees; verify crash windows rather than assuming network arbitration alone solves them. No permanent unbounded per-player or per-world history is required: finalized status stays in the fixed find record, and only unresolved receipts stay in the small journal.

Carried finds retain provenance through death, stash, tool conversion and display. A consumed/processed object's record cannot be restored as a free find by stale regional updates. A crafted collector's identity follows its owner's persisted equipment; reconnecting cannot create a second deployed copy. A departed player's deployed tool remains associated with that identity until retrieved or recovered through a defined session transition; never delete its contents as disconnect cleanup.

Food portions use stable recipe-batch/portion IDs and occupy exactly one inventory/cache representation. Persist preparation, valve yield and portion creation in one transaction. Each fixed crevice fixture record contains its chosen type, owner/revision and four bounded cache slots. Replacement must atomically preserve or move contents; no world-head append-only cache list. Salvaging award bits live in each fixed find/configuration claim record plus the player's skill total; replay/reload cannot award the same outcome twice. Non-transport capstones persist a bounded district completion bit, awarded only after their actual geometric/encounter conditions, not an inventory delivery.

This is new dynamic-object persistence work. The terrain paging refactor does not automatically provide it, and the implementation package must budget for it explicitly.

Flow feasibility spike: sample existing flow in the intended first-site geometry, release representative shell bodies, and verify a clear recoverable trajectory under native/wasm. If cutting a channel does not materially change local flow, do not advertise fluid routing: retain slope/outlet routing using actual terrain and place the known current lane explicitly. The content budget must not rely on an unbuilt pressure/fluid solver.

## Ordered implementation packages

1. Finish the opening physical slice and the existing common-site, food and shortcut integration. Preserve the first-slice playtest. Review the current integrated branch before assigning unfinished pieces.
2. **Build E next.** One complete Canyon Drift Run in production geometry, with actual both-axis force sampling, inlets, two body profiles, a repositionable catcher, safe overflow, durable renewal and a fully depleted save. Deliver one runnable native/web preview. Exposed tuning controls and a synthetic scene alone do not satisfy this gate.
3. **Prove all three late pursuits.** Deliver a minimal D sorting yard with two guide positions, two possible output plans and renewable feed. Deliver a minimal F meadow with a solo-created trace, a distinct answer from a second player and a visible refuge print. Add the shallow-rim fight/avoid alternative and test all these on the same depleted save. Limit bespoke opening authoring to its existing first slice and integration fixtures until each late pursuit can be played and repeated.
4. **Build across depth in pairs.** Pair A/D on collection, food, loaded nets and the working yard. Pair B/E on defended positions, combat, meals and interception. Pair C/F on traversal, route making, refuge fixtures and expression. At the first joint review deliver at least three local encounters and one mechanically different capstone stage per district. At the next review deliver six local encounters and a playable capstone per district. An early district can reach its final ten only when its late partner also has six, a capstone and a demonstrated return pursuit. Each pair eventually supplies its original 20 locals, two capstones and six optional challenges.
5. Complete the remaining recipes, specialization fittings, map/home presentation, variant authoring and optional masteries inside those district budgets. Stage the first-time ending and Keep exploring at F's completed return circuit. A–C remain useful to returning players for accessible food, familiar animal practice and short routes. D–F get the deepest combinations and the fully open-ended authoring anchors.
6. Run fresh and depleted campaigns separately. Track engaged time by district, first-time versus repeat encounters, reason for returning, and alternatives tested with equipment fully owned. Report unbuilt districts honestly at every preview. A checklist or projection cannot mark the full demo finished.

Every feature package includes sim behavior, presentation, phone input, save/replay, relevant co-op coverage and a playable preview. The complete-campaign release depends on both the authored-duration check and the depleted-world playtest below.

## Acceptance tests and playtest decisions

Automated guarantees, not claims of fun:
- Buried find cannot be collected through sediment/rock; clear a reachable side and collection succeeds.
- Two distinct dig geometries at the first site produce distinct release/settling outcomes; neither silently awards loot.
- Dig never damages a find. Intentional processing yields exactly two grit and permanently removes the intact claim.
- Removing support releases once; restoring/supporting contact settles without jitter or duplicate events.
- Catch tests cover valid interception, wrong side, obstructed mouth, excess speed, wrong size, full bowl and full player inventory.
- Every site has a reachable recovery basin and a validated no-required-feat exit. Required progression materials have a recoverable alternate source or nonconsumptive entitlement.
- Loaded-net intermediate poses remain collision-valid; cut order changes the release outcome; reloading mid-sequence preserves it.
- Combat cover works for each attack used at that site; wall crossing hits fail while valid exposed attacks still connect.
- Native/wasm seeded command sequences yield identical durable state. Replay includes placement, scoop changes, processing and cut actions.
- Simultaneous two-player take/process/place requests have exactly one accepted shared result; each player's progression claim remains available independently.
- Region boundary, host/guest reload, disconnect during release, death carrying a trophy, and reconnect with a full collector preserve ownership and avoid duplication.
- Food tests cover recipe double-spend, exactly-once valve yield, cancelled eating, soft/dead rejection, health/reserve caps and carry limits. Fixture tests cover full-cache replacement, co-op revision conflicts and reload without deletion/duplication. Salvaging awards are once per stable outcome and normal fresh runs reach proposed ranks without farming.
- B/C/D/F capstones can be completed with no shell cargo; A/E transport does not become a global prerequisite for combat, food or field-building loops. Standalone combat encounters award existing XP/body rewards without a salvage claim.
- Phone portrait/landscape contextual controls remain operable without hiding Dig or enemy tells; no tiny drag targets.
- At the proposed awake-body cap, record frame time, sim cost and build size against current main. Report regression and fix material regressions before adding more bodies.

Human checkpoints:
- **First slice:** three fresh players, no verbal instructions. Can they explain why the shell moved, choose a second approach, and willingly try another catch? Record confusion and abandonment. Sample size is diagnostic, not statistical proof.
- **Each district:** observe a player choosing between at least two activities, successfully using its new capability, and telling a specific cause-and-effect story. If all ten encounters are described as “dig until it gives you stuff,” redesign the district.
- **Full demo:** at least three fresh full runs with event logs and observation. Separate engaged activity time from travel, menus, idle waits and repeated recovery. Target median 5–7 hours of voluntary meaningful content on the intended first expedition, with a shorter skilled route acceptable. A tested content shortfall blocks the five-hour claim.
- Check that at least two viable gear orders and two broad route orders reach the ending, and that co-op is fun without making solo feel incomplete.

## Explicit non-goals

Do not lower skill thresholds, shorten moulting or move existing rewards forward as a substitute for these activities. Do not add a new MMO economy, fetch-quest board, mandatory rarity treadmill, repair tax, timer-based farming, infinite new world, trading economy, fishing minigame, new species, daily reset scheduler, paid upkeep, procedural dungeon generator, full rope/rigid-body engine, pressure fluid simulation or storage rewrite to this scope.

XP, new skills and seeded optional rare finds are allowed. The restriction is using grind, random progression walls or earlier existing thresholds as a substitute for interesting activities.

The measure of success is that the player has several things they actually want to try, and their own execution changes what happens. Five hours must come from authored opportunities and useful combinations of mechanics—not from making the same delivery take longer.
