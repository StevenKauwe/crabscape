# Digging

Digging is the game's second verb, after swimming: the way a crab reads, uses and survives the ground. This page is
the design for it and for the levels built around it. Engine facts (grains, pebbles, support, saves) are in
`docs/spikes/underwater-physics-design.md`; this page is only about what the player does and feels.

## Why it belongs at the core
The game is a dive where every descent is a debt repaid on the way back up (`vision.md`). Swimming spends stamina
going up. Digging is the other half of that economy: **a tunnel is an investment** (slow now, a shortcut for every
later trip) and **a burrow is a refuge** (hidden from what hunts by sight). Both answer "how far can I go and still
get back?", so digging is not terraforming for its own sake.

## Three jobs, one verb
| Job | What you do | Why it is fun |
|---|---|---|
| **Find** | Dig where the ground gives a hint (siphon holes, a glint of shell, a smell wisp) and turn up clams, eggs, grit, relics | Foraging is a guess that pays off in seconds |
| **Travel** | Tunnel through soft strata: down as a quick descent, up as a costly climb, sideways to skip a hazard | A route you made yourself, and keep |
| **Hide** | Bury yourself so sight-hunters lose you | A real trade-off against the lantern and the smell system |

Biology: crabs and other crustaceans do bury themselves in sediment, and bivalves live buried; mantis shrimp live in
burrows (not verified here; to be sourced before any number is tuned on it, per `ocean-shaped.md`).

## What makes it feel good
- **Strokes with a rhythm, like swimming.** Hold dig and a direction; the crusher shovels one load per stroke
  (about a quarter second), flinging a visible arc of grains behind the crab. Mashing does not dig faster.
- **Weight.** Loose silt gives way fast; sand flows back in unless it is held; pebbles are heavy and take effort;
  clay is slow but holds a tunnel. You feel the material in the stroke speed, the sound and the shake.
- **A reward inside ten seconds.** The first dig in the game always turns something up (a clam under a siphon hole).
- **A cost you can see.** Digging draws on the same reserve as sprinting; a spent reserve means you dig slowly.
- **Readable ground.** Diggable strata are drawn grainy, with ripples; bedrock is smooth and ink-lined; clay is
  cracked. No full-frame filter: the shapes tell you (`art-direction.md`).
- **A plume.** Digging raises silt that blocks your lantern and shows where you are to anything that hunts by sight,
  and drifts with the current. Hiding is therefore a decision, not a button.
- **Tunnels have a tell before they fail.** Loose strata trickle grains and rumble about a second before a stretch
  slumps (dodge out, or shore it up). Never an instant trap: you can always dig up.
- **The floor looks back.** Burrowing animals (the mantis, the moray, sand shrimp) hide in the same strata. A mound
  that is slightly wrong, or a stray grain trickling, is the tell. Learning to read the ground is how you get good.

## The strata language
Every region is three layers, and the layers are the whole level-design vocabulary:
1. **Frame:** bedrock and authored walls. Never diggable. It holds every gate, so no tunnel can skip an ability.
2. **Strata:** the diggable bands, by material (silt, sand, pebbles, clay), each with its own speed and behaviour.
3. **Finds:** what is buried in the strata: food, eggs, grit, relics, and some shortcut keys.

Rules that keep the levels honest:
- There is always an above-ground route. Digging gives a cheaper-but-slower route, a hidden reward, or a shortcut,
  never the only way, except where a gate is explicitly a digging gate (below).
- A dig can never bypass an ability gate. Each gate is bedrock, or sits behind bedrock, and a test enforces it.
- Dug edits are shared in co-op as commands and saved as a small list of changed cells, not the whole grid.
- **Silting:** unshored tunnels slowly fill with sediment from the current (a few minutes of play), so the world keeps
  changing and saves stay small. Tunnels **shored** with shell grit or clam valves (`craft.rs`) stay open for good.
  The home's grit is the same material: building the home and shoring a tunnel are one habit.

## Progression
- **From the start:** a scoop. Shallow digging in loose strata, enough for the first clam and a short hide.
- **Moult feat, burrowing:** bury fully (a crab-shaped hollow, only eyes and antennae out), which makes you invisible
  to sight-hunters while you stay still. Smell-hunters still find you, and upstream you are harder to find
  (`crab-memory.md`, smell rules). Biology of the feat: not verified.
- **Gear:** shell-grit shoring (stable tunnels) and the crusher's smash (cracks tagged brittle rock, not bedrock).
- Hunters get a counter-roll: sight-hunters (mantis, alarm jelly) lose a buried crab; smell-hunters (moray) close in
  on one, so you tunnel away across the current or fight; the octopus's arms feel the ground, so a tunnel gives away
  where you are by its tremor.

## Levels around it
Each region teaches one thing and gives one new way to use the ground:
1. **Shallows (teach).** A sand flat with siphon-hole clams: the first dig. Snapping shrimp feel footsteps (120
   units), so a buried or still crab passes: "being still is hiding". A shallow tunnel under the low ledge: the
   first shortcut.
2. **Trench.** The trench walls stay bedrock; its floor has a silt bed with the moray in a crevice. Hide-and-tunnel
   against a smell-hunter. The double-jump route is still the main way over.
3. **Basin and pit (descent debt).** The pit's silt walls make digging down a fast descent (and dangerous: slumps).
   The kelp line is the free way up. Digging an up-tunnel from the pit's passage to the basin turns the pit into a
   round trip you can repeat cheaply (shore it to keep it).
4. **Canyon (the current as terrain).** Sediment mounds drift with the flood and ebb; a lee behind rock is a place to
   hide from the current as well as hunters. Sea pens on the floor glow in answer to being disturbed.
5. **The octopus's den.** A real den sits in a bed of discarded shells (`world.md`). The arms feel the ground: the
   fight is about where you tunnel, because a tunnel under the den reaches its back wall, and a tremor tells it where
   you are.
6. **Later regions** reuse the vocabulary: a mudflat of clay (holds any tunnel, slow), a whale-fall bone field (dig
   among bones), a hadal silt plain (deep, soft, full of buried things).

## Tests that protect the design
- **No gate bypass:** for every authored gate, a crab with no ability but digging cannot get past it.
- **Never trapped:** from any dug position a crab can dig or swim up to open water with the starting reserve; slumps
  leave an escape (tell first, then collapse, never a sealed pocket).
- **Hiding:** a buried, still crab is lost by sight-hunters and not by smell-hunters within range; upstream/downstream
  cases hold.
- **Silting:** unshored edits fade, shored edits do not, and the saved edit list stays bounded.
- **Round trip:** a scripted crab digs down and back up a pit and the cost is within the reserve it started with.
- **Determinism and co-op:** the edit layer replays the same in native and wasm; two devices applying the same dig
  commands reach the same terrain; a friend's dig is visible to everyone.
- **Frame budget:** the cell step stays under its time budget on a phone-sized profile; a slow step is a failing test.
- **Pacing:** the first dig finds something within ten seconds of starting to dig.
