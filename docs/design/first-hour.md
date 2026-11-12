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
