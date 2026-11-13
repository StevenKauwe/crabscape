# The first hour: levels 1 to 15 for a brand-new crab

Owner direction (2026-11-12): the game needs a hella fun climb from level 1 to 15 in **every** skill, for a new
crab. The 20-minute loop (`core-loop.md`) was tuned for a level-30 crab (the lamp needs Harvesting 30), so a new player
had no designed first session. New regions are frozen after region 6; the long-term map stays in the region plan.
This page is the target for sessions 1 to 3. The **world lane** authors the places and sites; the **systems lead**
owns the progression: the unlock ladder, recipes, the task board's first-hour chain, XP tuning, and a level-1 bot that
measures it.

## The target
- A new crab reaches about **level 15 in its main gathering skill around minute 20** of its first session.
- **Every skill** (the five gathering skills, Fighting, Climbing, Swimming, Building, Cooking, Weaving and the rest)
  reaches 15 over the **first few hours of mixed play**, without grinding one node.
- **No single node is worked for more than a few minutes**: the next thing is always a short walk away, and different.
- **The guardrail stays:** the length comes from the world (places, routes, things to find), never from padding
  (slower gathers, higher costs, repeat trips down the same line).

For scale: on the RuneScape table, level 15 is 2,411 XP. Today a tier-1 gather pays 10 XP in 2.5 s, so 15 takes 241
gathers (about 10 minutes of pure holding, and the nodes' regrowth makes it longer). Reaching 15 by minute 20
therefore needs both the systems lead's tuning (what a first gather, a first visit, a first kill or a task pays) and
the world's variety (enough different nodes on the way that a crab never waits for one to regrow).

## What the classics do (and what we take)
- **RuneScape (Tutorial Island, then Lumbridge).** Each skill is shown once, in a few minutes, by doing it; then the
  first levels come every minute or two, because the XP table is shallow at the bottom (level 2 is 83 XP, 10 is
  1,154). Many small, cheap first unlocks (a new tree, a new fish, a new recipe every few levels) make each level-up say
  something. *Take:* a level-up every one to two minutes early, and each one opens a named thing you can see nearby.
- **Valheim's Meadows.** Gather by hand, make a first tool, build a fire and a roof, hunt deer, and their trophies call
  the first boss, whose drop opens the next biome. Everything you need is in one gentle biome; the next biome is
  visible from it. *Take:* the first hour lives in one gentle area with a visible next step, and a creature's part
  (the first shrimp claw) is a key, not just loot.
- **Stardew Valley's first days.** Small energy, a short list of small goals (clear the farm, plant parsnips, meet
  people), and a day ends with a visible change. *Take:* end each session at home with something built.
- **Hollow Knight's Forgotten Crossroads / Silksong's Moss Grotto.** A small connected area with a bench as its hub,
  a loop back to it you open yourself, landmarks you can name after one visit, and glimpses of places you cannot reach
  yet (a ledge too high, a door that needs a tool). *Take:* nameable landmarks, a shortcut you open, sights of later.

## Beat sheet

### Session 1 (minutes 0 to 20): home, the shallows and the shelf
| Minutes | Beat | Where (world) | What a new crab learns and earns |
|---|---|---|---|
| 0–2 | Wake at home; a task says "gather three things" | Home and its two pockets | One gather in each of the five skills, in sight of the door. First level-ups. |
| 2–5 | The first fight | Shrimp row (the burrows at x 340 and 590) | Read a tell, win, take a claw: the first Fighting levels and a part that makes the first tool. |
| 5–9 | Up the shelf | The tide-pool shelf (x 720 to 1,250) and its pools | Pools and rocks with nodes of every skill. The first climb, up a kelp stand to the lookout, pays a node and a view. |
| 9–13 | The first dive | The trench (x 1,330 to 1,800, floor -260) | Drop in, gather on the rim and the floor; the silt mouth glows below: the first glimpse of the Silt Galleries. Tier-2 nodes visible on the trench floor: "Prying 15 opens clams". |
| 13–17 | The far ledge | x 1,850 to 2,470 | A line of tier-1 nodes past a snapper; the basin's drop seen past it (the turtle overhead). |
| 17–20 | Home and build | Home | Unload, make the first tool and a first home piece. Main gathering skill about 15. |

### Session 2 (about 20 to 60 minutes): the first basin and the Silt Galleries' mouth
- Level 15 opens tier 2: the trench floor, then the basin. The silt mouth is the first cavern, with its rest point.
- The first climbs out of the basin by its return ledges; a hunter (the moray, the hermit crab) between you and home.
- Wants that need a creature part: the second and third fights.

### Session 3 (about 1 to 3 hours): every skill to 15
- Mixed trips that touch every skill: Cooking and Weaving from what was gathered, Building at home, Climbing and
  Swimming from the routes themselves, Culturing at the glow sites.
- The upper gallery's glass (Harvesting 30) is the next long want: the first 20-minute loop of `core-loop.md`.

## World lane: the sites (to build, in this order)
Tier 1 lies above y -260 (the shallows band) and tier 2 between -260 and -700 (`level/bands.rs`). The world lane names
the places and gives positions; the systems lead places the nodes and their kinds.
1. **Every place on the way out has a name a new player can say:** home, shrimp row, the shelf, the pools, the
   lookout, the trench, the silt mouth, the far ledge, the basin.
2. **More, smaller, mixed groups of tier-1 nodes** along x 0 to 2,800, so no skill has to wait for regrowth: at
   least six groups of two to four nodes of different skills, at most about 400 units apart, each out of every
   hunter's reach. Today there are three lines (x 222 to 242, 432 to 492, 1,850 to 2,470).
3. **Shelf pools:** two or three shallow dips in the tide-pool shelf (a crab drops in and climbs out by itself), each
   holding a node or two: the first small dive, out of reach of the snapper.
4. **The lookout:** a rock pinnacle on the shelf, climbed by a kelp stand, with a node on its top and a view east over
   the basin to the canyon (a sight of later).
5. **The trench floor** holds a few tier-2 sites, visible from its rim: a level-1 crab sees them and the skills panel
   names the level they open.
6. **Small finds** (the shore's drifting finds and buried things) along the way, a few per hundred metres.
7. **Rest points:** home, and the silt mouth's (already one).

Measured targets (a level-1 bot, the systems lead's): the five skills each reach level 5 within the first 10 minutes,
the main gathering skill about 15 at minute 20, and every skill 15 within three hours of mixed play.

### Built (the first-hour world PR, `level/shallows.rs`)
- Names: "shrimp row", "the shelf", "the trench", "far ledge", "the crest", "the lookout",
  "trench hollow".
- **No pool yet.** A ledge pool at the foot of the far ledge trapped a crab walking east in the cells world, and a
  shallow one the bake fills with sand; the trench is the first dive. The shelf could not take one either (the
  arena of several creature and economy tests).
- **The lookout** (x 330 to 490, top y 704, 160 wide), stepped onto from the kelp stand by home: the first climb, a site on top
  and a view.
- **Trench hollow** (x 1,380 to 1,520, y -480 to -340): a bay off the silt mouth's shaft, below the tier-2 line,
  holding the first tier-2 sites a level-1 crab sees.
- `shallows::SITES`: ten mixed tier-1 groups from x 234 to 2,760 (gaps under 420 but the trench's ~600) and three
  tier-2 sites in the hollow; `shallows::FINDS`: nine small finds. The systems lead places the nodes and resolves the
  shelf sites against the baked ground (the shelf's sand settles about 100 lower than its authored floor).

### Built, part 2 (`level/shallows.rs`: weak creatures, short climbs, a paddle gap)
- **Two more snapping shrimp** (`shallows::FIGHTS`), each beside an old one: "row shrimp" at x 622 (by the shrimp at 590)
  and "block shrimp" at x 2,074 (by the one at 2,060). Creatures stand on the floor, and almost none is left that keeps
  92 off every node and site, off the shelf's creature arena (x 900 to 1,290), 294 off the plug shaft, and clear of the
  co-op walks, the rests and the moray; more shrimp needs nodes moved (see `decisions.md`). A bare level-1 crab beats each
  colony of two untouched.
- **Three shore crabs** (`shallows::SHORE_CRABS`, passive, walk home x +-120): the slope (x 800, walks 680 to 920), the
  shelf's east end (x 1,120, 1,000 to 1,240) and the far ledge's slope (x 2,230, 2,110 to 2,350). Walks clear the co-op dig
  spots, the rests, the home door, the plug shaft's mouth and the moray's reach; none on the trench floor (a rest crevice
  and a moray at each end). A bare level-1 crab beats each; a gatherer beside one is never hit.
- **Two kelp ledges** (`KELP_LEDGES`): "block kelp ledge" (stalk x 1,980 on the block's top, ledge x 1,732 to 1,892, top 460)
  and "flats kelp ledge" (stalk x 2,392, ledge x 2,480 to 2,640, top 200). Each is climbed with no upgrade and pays
  Climbing XP. (Nothing under y 664 fits in x 400 to 1,400: the cell-join tests' pebbles lie there.)
- **The paddle gap**: the lookout's east end to the "far lookout" (x 790 to 950, top 704, 160 wide), 300 apart and 600 above the
  floor. A jump alone carries about 275; one egg's stroke crosses. The far lookout holds two sites (ShoreGlow,
  SnailRocks) and a find. A miss lands on the egg ledge or the floor, unhurt.
- The boulder sites moved to x 695 and 710 (92 clear of the row shrimp); "the lookout" gained a neighbour, "far lookout".
- Co-op: the new shrimp reach x 550 to 694 and 2,002 to 2,146, clear of x 100..240 and 2,400..2,540. Crabs walking east
  from home pass the row shrimp (x 622) and the block shrimp (x 2,074); the flats ledge's stalk (x 2,392) stands just
  west of the cells check's dig at about x 2,470.

### Built, part 3 (`level/shallows.rs`: sights a new crab sees and cannot reach yet)
- **The high garden** ("high garden": ledge x 760 to 940, top 1,300; a canopy slab x 350 to 1,250, underside 1,530, over
  it). In plain sight over the shelf from the lookout, the far lookout and the floor. The kelp stand by home climbs
  to the surface, and a crab that kicks off it drifts east about two units per one it sinks, so a bare ledge would have
  been a free glide; the canopy leaves 165 to sink under it, so only strokes carry the crab. Measured (route solver
  from home, both lookouts and the floor): 0 and 1 eggs never rest on it; **2 eggs do** (stalk climbed in steps to
  1,224, kick off east, two strokes, then a hop). Two sites (`shallows::SIGHT_SITES`, apart from `SITES` until wired) for a tier-3 want ("GlassSpongeGarden", "TwilightGlow",
  place "high garden") and one find. The existing band rule (tier 1 above y -260) is waived for this place only.
- **The glow ledge** ("glow ledge": ledge x 1,488 to 1,648, top 1,100, east of the sea arch over the trench's west rim)
  and its **glass stair**, a bare rock stalk (`Growth::Rock`) at x 1,400 from y 60 to 1,140. Only glass grips climb it:
  a glass-gripped crab with no egg reaches the ledge from home (walk to the rim, jump to the stalk, climb in steps,
  kick off east). With no glass, 0 or 3 eggs never reach it from the rim, the trench, the block or the far ledge (a
  fourth egg's strokes from the block kelp's top do). Two tier-1 sites (ShoreGlow, SnailRocks) and a find.
- Both are named, in `bands.rs` and in `tests/shallows.rs` with both halves proved (about 90 s for the file).
### Built, part 4: look and signposts (client drawing only: `src/signposts.rs`, `src/outcrop.rs`, `src/flats.rs`)
- **The guide's glint.** Nodes of the kind the local crab's current guide step gathers (`Cond::Gather`) sparkle slowly
  on their own shape over a warm light pool on the ground; spent nodes, a skipped or finished guide and remote crabs'
  guides show nothing.
- **Sights of later.** A node whose level is above the local crab's skill shimmers a quiet cool blue (never warm), so the
  level-8 and level-15 spots read as "something there". The trench hollow's nodes glow sea-green and the glow spills out
  of its east door into the silt mouth shaft (seen from the trench floor); the plug shaft ledge's pit glow is teal and
  lights the shaft's walls up and, fainter, 500 down to the shaft's foot. Soft glow sprites, as the far lights down the
  crack (`secrets.rs`): no shader, nothing over the screen.
- **Ledges as outcrops.** `canopy.rs` is now `outcrop.rs`: the canopy and every floating ledge on the route (both
  lookouts, the two kelp ledges, the glow ledge, the garden's ledge) get ragged rock with sponge, sea squirts and swaying
  fronds under it. The top stays flush (a lip under 6 above the collision top), the underside hangs at most 30.
- **Placeholders.** The silt gallery plug is wedged rubble (cobbles and packed silt in its own rect, gone when cut); the
  glass stair is a slender spire of painted rock with glassy glints.
- **Flat stretches.** Shell hash, small anemones, sea stars and pebbles on the shelf's top and the far ledge, low in
  contrast, placed from position alone, 20 clear of every node. The shelf's top holds about 2.8 per 200 units and the far
  ledge 2.6 (the brief's 3 to 6; the keep-off leaves little free ground between the nodes).
- Tests: `outcrop` (limits for each slab, none without shallows), `signposts` (glint set is exactly the guide's kind near
  home, follows the step, off when skipped; level-8/15 shimmer; both hidden places glow), `flats` (clear of nodes, none
  without shallows).

## Variety audit: levels 1 to 15 in every skill (2026-11-13)
Where a new crab trains each skill between levels 1 and 15, what the ladder (`economy/ladder.rs`) opens on the way, and
how the method changes. "Places" are near home unless named. The bar (coordinator, 2026-11-13): at least two distinct
places or methods by 15, and something new around level 5 or 10, so no skill is the same click for 15 levels.

| Skill | Where (levels 1 to 15) | Opens on the way | How the method changes | Gap before this audit |
|---|---|---|---|---|
| Climbing | Kelp stalk by the door (to the lookout, 704 up, and the surface); block and flats kelp ledges; the silt mouth's coral stalk; the plug shaft (a 1,000-unit climb, ledge halfway) | +8% climb speed a level; gripping dactyls ready at 5 | Short stalks, then ledges you step off, then the long shaft climb with a rest | None |
| Swimming | The egg over the first pocket; the paddle gap (lookout to far lookout); strokes along the way out | Strokes 5% quicker a level to 10, then legs rest quicker; broad paddles ready at 5 | One stroke, then a gap to cross, then the high garden (two eggs) as the sight of later | None |
| Fighting | Snapping shrimp (x 340, 590, 622; 2,060, 2,074); shore crabs (x 800, 1,120, 2,230); morays (trench x 1,760, flats x 2,650); the basin's mantis (x 3,880) | Claws 5% harder a level to 10, then 2% more; heavy crusher ready at 5 | Passive shore crabs, then shrimp colonies of two, then a moray, then the mantis | None |
| Foraging | Snail rocks in every group; brittle-star crevice (8); worm mud (15: trench hollow, basin) | Quicker picks, second-item chance, brittle stars at 8, worms at 15 | Pick by hand throughout | Level 8 node only in the home pockets |
| Prying | Mussel clumps in most groups; barnacle rock (8); clam bed (15: trench hollow, basin) | Quicker, second item, pearl chance 1 in 64 to 1 in 40, barnacles at 8, clams at 15 | Pry tip from step 9; better tips after 15 | Level 8 node only in the home pockets |
| Excavating | Grit heaps; digging the cells themselves; sand-dollar bed (8); clay bank (15: trench hollow, basin) | Quicker, second item, sand dollars at 8, clay at 15 | Scoop, plus real digging (the plug from below) | Level 8 node only in the home pockets |
| Harvesting | Kelp stands; eelgrass bed (8); sea-pen field (15: the basin, x 4,500) | Quicker, second item, eelgrass at 8, quills at 15 | Cutter throughout | Level 8 only at home; the first quill is the basin's (the loom, Building 10, takes valves, not quills) |
| Culturing | Shore glow; purple film (8); pit glow (15: the basin); home culture beds (bed 1, Building 5) | Broths at 2 and 4, beds 10/20/30% quicker at 6/10/14, starter at 9, glow seed at 12, plant culture at 15 | Take wild strains, then plant and tend beds at home | Level 8 only at home; pit glow only in the basin |
| Cooking | Workbench; brine pot (Building 10) | A dish at nearly every level, 3-portion batches at 4, 9, 14 and 15 | Workbench dishes, then the brine pot (worm paste, 12) | Built: the mussel and snail hash (6) and the barnacle and kelp roll (14) take their place; the jelly salad and inked kelp move to 18 and 19 |
| Shellcraft | Workbench | Polished valve (2), cement (4), plating (5), sand polish (8), barnacle cement (9), plating mend (11), doubled blanks and polish, tools last longer, tier-2 tools at 15 | Workbench, then the grindstone (Building 14): ground valves at 14 | Built: a second method at 14 |
| Weaving | Workbench | Rope (2), mat (4), quick twist (5), sack (8), eelgrass cord (9), slime twine (10), eelgrass mat (12), rope coil (14), doubled cord, rope and mats | Workbench, then the loom (Building 10): loom-woven mats at 11 | Built: a second method at 11 |
| Building | Home (the Home tab) | A piece or station at every level: valve step (2) to barnacle wall (15); bed 1 (5), brine pot and loom (10), grindstone (14) | Pieces, then stations that change other skills' methods | None |
| Survival | The twilight band (below y -700): the upper gallery down the silt mouth (y -1,040), the basin's deep end, the canyon walls | 1% less pressure strain a level, 2% to 15% | Sitting deep; twice as fast under strain | Built: at 5 a meal keeps you well fed 15% longer; at 10, 30% longer, and the strain starts 200 deeper |

### What the world adds (built)
- **Level-8 places of their own.** Each half-tier node also grows in one place out on the route that fits it, so level 8
  sends a crab somewhere new rather than back to the home pocket: barnacles on the far lookout's wave-washed top,
  brittle stars in the cracks of the boulder half-buried in the slope, purple film in the shade under the sea arch at
  the trench rim, eelgrass on the far ledge's slope and sand dollars on the sand at the far ledge's foot. The home pocket ones stay.
  Sites: barnacle rock x 868 (far lookout), brittle-star crevice x 720 (the boulder's east edge), purple film x 1,246
  (trench rim), eelgrass x 2,310 (slope), sand dollars x 2,385 (foot).
- **Tier 2 for Harvesting and Culturing near home.** A sea-pen field in the trench hollow's mud, and a pit glow site on
  the plug shaft ledge (the dim niche halfway up the plug shaft, reached from the upper gallery). Level 15 in either
  skill then has two places (near home and the basin), and the loom's quills come within one trip.
  Sites: sea-pen field x 1,435 (hollow floor -480), pit glow site x 1,120 (plug shaft ledge, floor -540).

### For the systems lane (built)
- Survival: something at 5 or 10 beyond the percentage (a longer stay before strain, or slower hunger, as the
  skills table promises).
- Shellcraft and Weaving: a second method before 15 (a station or a place to work), so neither is only the workbench.
- Cooking: the jelly salad (6) and inked kelp (14) name far creatures; a near source or a near swap.

## Playtest: what to try first (owner, 15 to 20 minutes)
Start a **new crab** (a fresh save, or a private window on the web) and follow the guide's pill. The bot finishes the
17 steps by about minute 15 on real ticks (`first_hours.rs`); a person reading the cards will take longer, and that is
fine. Keep a timer running and jot the minute you reach each block, along with anything that made you stop and think.

| Minutes (bot) | Do | Notice |
|---|---|---|
| 0–4 (steps 1–5) | Follow the pill and the amber diamond out of the door: cut kelp, forage snails, then pry mussels past the first snapping shrimp, scoop grit, gather shore-glow. When the shell fills (4 things), the pill says "shell full, take it home" and the diamond moves to the door. | A level-up every half minute or so, and each card says what it opened. Does the pill always say what to do next and where? Does the diamond help, or get in the way? Is the trip home when the shell fills clear? |
| 4–6 (steps 6–11) | Home to the workbench: kelp cord, shucked mussels (eat one), a shell blank, the pry tip, then build into the home and the valve step from the Home tab. | Whether the three workbench tabs make sense (Goals, Craft, Home). The tool fits the claw on its own. The valve step appears at the door: the first visible change to home. Guide cards here hand you the inputs for the next step. |
| 6–9 (steps 12–13) | Climb the kelp stalk beside the door, eat the egg at the top, and paddle a stroke over the first pocket. Then try the lookout ledge up the stand (y 704) and the paddle gap to the far lookout. | Climbing and Swimming level fast here (they pay ×2.2 below 15). Does the paddle gap read as a jump you can make? The lookouts hold nodes the pockets don't. |
| 9–11 (step 14) | Beat a snapping shrimp: watch its tell, dodge the shock, smash it while it recovers. Three stand on the home side (x 340, 590, 622). A shore crab walks the slope at x 680 to 920: it never starts a fight, so it makes a gentle first one. | Whether the tell is readable on the first try, and whether one fight feels worth it (a level or two at first). Note any death and its cause. |
| 11–13 (steps 15–16) | Go east over the shelf, past the trench rim, and down the trench into the basin band. Look into the trench hollow (tier 2: a clam bed, clay bank and worm mud you can't work yet). Then go deeper, into the twilight, for Survival. | The first "I can't do that yet" sight: does the skills panel tell you the level that opens it? Is the way down obvious, and the way back up? |
| 13–15 (step 17) | Home, open the Goals tab and pin a want. | The guide ends there. Is there one clear thing to do next, or a list that's too long? |
| 15–20 | Free play: do whatever the pinned want asks, or try the far ledge past the trench (two more shrimp at x 2,060 and 2,074) and the kelp ledges there. | Whether you still know what to do without the guide, and how long it is between level-ups now. |

Try the **Skip guide** button once (Goals tab), then turn the guide back on. It should resume at the first step you
haven't done.

**What we want to know** (a line each is plenty)
- **Fun:** which minute was the best, and which the dullest? Did any level-up card make you want the next one? Was the
  first fight fun or a chore?
- **Confusion:** where did you stop to think about what to do or where to go? Which card, tab or name didn't make
  sense? Did you ever lose the pill?
- **Pacing:** did levels come too fast (cheap) or too slow anywhere? Did you work one node long enough to get bored?
  Did the guide's 17 steps feel too many, too few, or about right?
- **Wants:** after the guide, did you want something on the Home tab or in the recipes enough to go out for it?
- **Co-op (if a friend joins):** does the guide stay yours, and does their play ever move your steps?
