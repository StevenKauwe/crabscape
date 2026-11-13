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
- **The lookout** (x 368 to 458, top y 704), stepped onto from the kelp stand by home: the first climb, a site on top
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
- **Two kelp ledges** (`KELP_LEDGES`): "block kelp ledge" (stalk x 1,980 on the block's top, ledge x 1,802 to 1,892, top 460)
  and "flats kelp ledge" (stalk x 2,392, ledge x 2,480 to 2,570, top 200). Each is climbed with no upgrade and pays
  Climbing XP. (Nothing under y 664 fits in x 400 to 1,400: the cell-join tests' pebbles lie there.)
- **The paddle gap**: the lookout's east end to the "far lookout" (x 758 to 848, top 704), 300 apart and 600 above the
  floor. A jump alone carries about 275; one egg's stroke crosses. The far lookout holds two sites (ShoreGlow,
  SnailRocks) and a find. A miss lands on the egg ledge or the floor, unhurt.
- The boulder sites moved to x 695 and 710 (92 clear of the row shrimp); "the lookout" gained a neighbour, "far lookout".
- Co-op: the new shrimp reach x 550 to 694 and 2,002 to 2,146, clear of x 100..240 and 2,400..2,540. Crabs walking east
  from home pass the row shrimp (x 622) and the block shrimp (x 2,074); the flats ledge's stalk (x 2,392) stands just
  west of the cells check's dig at about x 2,470.
