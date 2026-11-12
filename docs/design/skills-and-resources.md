# Skills and resources: the RuneScape loop

The spec for CrabScape's skills and resource economy (issue: see "Packages"). Self-contained: a mechanical implementer
can build any package from this page plus the files it names. Owner's wish: "a much deeper set of skills and resource
collection, with a very RuneScape-esque feel." The loop is **collect resources → level up skills → improve crafting →
repeat**, and **the further from home, the better the resources.**

Read with: `vision.md` (Grows like RuneScape), `world.md` (Zones and economy, "Things get used up"), `ocean-shaped.md`
(everything real, no air, no fire), `universe.md` (the map by depth), `coop-promises.md`, `world-plan.md` (the world
build running alongside: its hidden rules apply here too).

## The feel in one paragraph
You leave home with an empty shell and a shell pry tip. Two kelp stands and a mussel clump sit within sight of the
door; in a minute "Prying level 2!" pops by the crab, its claws lift and a soft chime rings. The shell fills (four
mussels cling to it), you walk home, tuck in, and the workbench takes them. You shuck two into food, braid kelp into
cord, and with cord and two valves you shape a better tool. The skills panel says clams open at Prying 15, down in the
basin. Ten minutes later you stand at the trench edge, deciding whether a full shell of clams is worth the climb back.
That decision, every trip, deeper each time, is the game.

## Rules that shape everything (anti-patterns first)
| Do | Never | Why |
|---|---|---|
| Rules in `crates/sim` (fixed ticks, no clocks); `src/` only presents | Gather timers, XP or respawn in a Bevy system | Replays and co-op must reproduce them |
| Chance from a hash of the crab's own counters (`economy::roll`) | Drawing from `Sim::rng` for a personal outcome | A guest's personal roll would desync the host's shared RNG |
| Nodes are **shared** world state (`Shared`), taken through the host referee (`Thing::Node`) | A node's charges stored per crab | `coop-promises.md`: resources are shared, run by the host |
| Skills, XP, tools, gear, bank, culture beds, buffs are **personal** (`SimCrab::econ`, saved in `Progress`) | Changing a `remote` crab's econ | Each device owns its crab |
| Place nodes by **anchor** (a named region box + floor snap) in `economy/place.rs` | Editing `Level` fields, `level.rs`, the floor or rocks | Any floor/rock edit forces a 9-minute rebake, and the map is moving under the world build |
| Derived data as methods (`economy::place::nodes(&Level)`) | New fields on `Level` | `Level` derives `PartialEq`/serde (world-plan hidden rule) |
| Real organisms at real depths, sourced or "not verified" | Fire, smelting, ovens, air, magic ores, made-up species | `ocean-shaped.md` |
| A level-up says what it unlocked ("Prying 15: you can open clams") | A silent number change | RuneScape's joy is the unlock |
| Level-up juice on the crab: claws lift, a short glow ring, a chime | Full-screen flash, overlay or filter | Rules that never bend |
| Common resources stay useful (repair, cord, food, building) | A tier that is dead once you pass it | "Things get used up" |
| Every trip ends with a decision (full shell, go deeper or home?) | Unlimited carrying | Inventory pressure is the loop |

Good example: `Prying 30 → glass scallops on the canyon shelf (x 5,900 to 6,150, y about -3,400)`: 160 XP each, needs a
quill pry tip, 4 per shell; the shelf crevice is a refuge, open water around it is frightening. Bad example: a "rune
ore" glowing rock in the shallows worth 500 XP (made up, no risk, no distance).

## Skills (13 + the royal side skill)
All skills use one **RuneScape experience table** to a cap of **99**: `xp(L) = floor( Σ_{l=1}^{L-1} floor(l + 300 ·
2^(l/7)) / 4 )` (level 2 = 83, 5 = 388, 10 = 1,154, 15 = 2,411, 20 = 4,470, 30 = 13,363, 45 = 61,512, 60 = 273,742,
92 = 6,517,253, 99 = 13,034,431). The table is computed once as a `const` array (`economy::xp::TABLE`).

| # | Skill | RuneScape analogue | Grows by | Each level gives |
|---|---|---|---|---|
| 1 | **Climbing** (exists) | Agility | Climbing | +8% climb speed (cap 10, as today) |
| 2 | **Swimming** (exists) | Agility | Paddle strokes | Quicker strokes (cap 10, as today) |
| 3 | **Fighting** (exists) | Attack/Strength | Claw damage, kills | +5% claw damage (cap 10, as today) |
| 4 | **Foraging** | Fishing (hand) | Picking up loose food: snails, worms, fish falls, carcasses | Faster gathering, unlocks tiers |
| 5 | **Prying** | Fishing | Prying shellfish off rock and out of mud | Faster, tiers, pearls |
| 6 | **Excavating** | Mining | Digging buried finds in sediment (nodes; real digging earns too) | Faster, tiers |
| 7 | **Harvesting** | Woodcutting | Cutting kelp, sea pens, sponges, deep coral | Faster, tiers |
| 8 | **Culturing** | Farming | Taking wild bacteria; planting and harvesting home culture beds | Tiers, bigger bed yields |
| 9 | **Cooking** | Cooking | Preparing food without fire: shucking, curing in clay, fermenting, vent poaching | Recipes, fewer spoiled batches |
| 10 | **Shellcraft** | Smithing | Shaping shell, quill, glass, bone, nodule into blanks, tools and plating | Recipes |
| 11 | **Weaving** | Crafting/Fletching | Kelp into cord, braids, sacks and packs | Recipes (bags = more carrying) |
| 12 | **Building** | Construction | Building into the home: stations, culture beds, the shell | Stations, beds |
| 13 | **Survival** | Hitpoints/Prayer-ish | Time spent cold, deep and under pressure (band ≥ twilight) | −1% pressure strain per level (cap −40%), slower hunger |
| (14) | Data-Driven (royal) | n/a | The royal observatory | Unchanged: keeps its own quadratic curve, listed last |

Existing skills migrate to the RuneScape table; their earn rates are rescaled so **level 5 (the moult feat level) lands
at the same activity as today**: Climbing ×1.94 (0.0194 XP per unit), Swimming ×3.88 per stroke, Fighting ×0.97. Their
effects keep `Tuning::skill_cap` (10). The moult feats keep `feat_level` 5. Existing tests that assert old thresholds are
updated (say so in the PR).

**Storage.** The three existing skills keep their `Mover` fields (`climb_xp` etc., f32). The ten new skills live in
`SimCrab::econ.skills: SkillBook { xp: [u32; 10] }` (whole XP; every award is a whole number; Survival and Excavating
by-the-unit awards keep an f32 remainder in `SkillBook::carry: [f32; 10]` that is not shown). Saved via
`Progress::econ` (`#[serde(default)]`). One API for all: `Sim::earn(id, Skill, xp) -> Option<LevelUp>` and
`SimCrab::level(Skill, &Tuning)`.

**Level-up.** `LevelUp { skill, level }` goes into the crab's pending events (`StepEvents::level_up`), a sound cue
`Cue::LevelUp { crab, skill, level }`, and the HUD notice "Prying level 15!" plus the unlock line from
`economy::unlocks(skill, level)` (every node, tool, recipe and station whose requirement is exactly that level).

## Tiers: depth is wealth
Five tiers, one per depth band (`level/bands.rs`). A node's tier sets everything about it.

| Tier | Band (y) | Places on today's map | Level req | Tool tier | XP per gather | Gather s (base) | Charges | Respawn s | Nodes per kind |
|---|---|---|---|---|---|---|---|---|---|
| 1 | Shallows (> -260) | home side pockets x 222–242 and 432–492; far ledge x 1,850–2,470 | 1 | 0 (bare claw) | 10 | 2.5 | 5 | 45 | 4 |
| 2 | Basin and mudbanks (-260 to -700) | far basin hub x 4,500–4,940 for all five skills (the home burrow gallery holds none: lead's call, see Where things stand) (the first basin, x 2,800–3,300, holds none: a moray and a hermit crab) | 15 | 1 shell | 110 | 3.0 | 5 | 60 | 3 |
| 3 | Canyon walls / twilight (-700 to -3,400) | doorstep x 5,443–5,790 (none on the shelf); far rim x 7,905–8,015; the bone hall's floor (fish falls and tooth beds, y about -2,534, x 3,880–4,130: the chamber prize) | 30 | 2 quill | 160 | 3.5 | 4 | 90 | 3 |
| 4 | Canyon floor / midnight (-3,400 to -9,500) | canyon floor far end x 7,130–7,610 (y -6,000); the seep garden (seep mussels and the sulfur mat, y about -10,514, x 19,430–19,605) | 45 | 3 glass | 260 | 4.0 | 4 | 150 | 3 |
| 5 | Deep (< -9,500) | lower canyon vent field east end (`level::lower_canyon::VENTS`, x 19,730–20,450, west of the crust vault's door) | 60 | 4 bone | 750 | 4.5 | 4 | 240 | 3 |

Placement rule: a tier-k node lies in band k or deeper (by depth alone: a pocket does not change it, so tier 4 cannot lie in the
bone hall at about -2,500, and the hall holds tier 3), and its **route time from home** (route solver, the kit the world
expects at that stage) is longer than every tier-(k−1) node's median. The pit floor is deep but close: put only tier 2–3
there (its existing culture stays the unique lantern pickup), never tier 4–5.

**Gather time** (ticks) = `base_s × 120 × 100 / (100 + 2·(level − req) + 15·(tool_tier − (tier − 1)) + buff)`, never
below 40% of base. Deterministic: no fail rolls. A gather is a **hold**: the crab stands (grounded or clinging) within
70 units of a ready node and holds Gather; walking, jumping, dodging, a hit or letting go cancels and the bar empties.
Each completed gather takes one charge, gives one item (onto the shell; a big item onto the back) and the XP, and wears
the tool by 1. At 0 charges the node is spent and regrows all its charges after its respawn time (the host's clock).

**Rare finds.** Each completed gather rolls `economy::roll(crab_identity, node_index, gathers_done) % 64 == 0`
(a deterministic hash; never `Sim::rng`) for a bonus: a **pearl** from any bivalve (Prying), a **fossil tooth** from
Excavating tier ≥ 2, a **scale** from Foraging tier 5. Pearls are real ("natural pearls form in bivalves", source to
add: not verified until opened).

## Nodes (data: `economy/nodes.rs`)
Five gathering skills × five tiers = 25 node kinds, plus culture sites (Culturing) and one station kind.

| Skill (tool) | T1 | T2 | T3 | T4 | T5 |
|---|---|---|---|---|---|
| Prying (pry tip) | Mussel clump → Mussel | Clam bed → Clam | Glass scallop bed → Scallop | Seep mussel bed → Seep mussel | Vent clam bed → Vent clam |
| Harvesting (cutter) | Kelp stand → Kelp | Sea-pen field → Quill | Glass sponge garden → Glass | Black coral → Black coral | Bamboo coral → Bamboo coral |
| Excavating (scoop) | Grit heap → Grit | Clay bank → Clay | Tooth bed → Tooth | Whale-fall bones → Whale bone (big) | Nodule field → Nodule |
| Foraging (none) | Snail rocks → Snail | Worm mud → Worm | Fish fall → Lanternfish | Grenadier carcass → Grenadier (big) | Scaly-foot snails → Scaly-foot |
| Culturing (none) | Shore glow → Shore strain | Pit glow site → Culture | Twilight glow → Twilight strain | Sulfur mat → Sulfur mat | Vent mat → Vent mat |
| Station | | | | | **Vent pot** (cook at the vent: see Cooking) |

Culturing wild sites give 8 / 88 / 160 / 208 / 600 XP (80% of gathering; beds pay the rest). Big items ride on the back
one at a time (like today's culture and tooth); everything else clings to the shell.

Biology (each row "not verified" until a source is opened and added to this page's Sources): mussels on shallow rock;
clams in mud; deep-water scallops (*Propeamussium*); seep mussels (*Bathymodiolus*) and vesicomyid clams at seeps and
vents; kelp in sunlit water only; sea pens on soft mud; glass sponges on deep hard ground; black corals (Antipatharia)
and bamboo corals (Isididae) deep; fossil shark teeth in sediment; whale-fall bones; manganese nodules on the abyssal
plain (`universe.md` [NOAA-MN], verified); grenadiers (rattails) as deep scavenged carcasses; scaly-foot snails
(*Chrysomallon squamiferum*) with iron-sulfide scales at vents; luminous bacteria; sulfur-oxidising mats (*Beggiatoa*)
at seeps. The crab does not "cook": it shucks, wraps in clay, ferments with bacteria, and poaches in vent water
(warm vent water is real; that crabs use it: not verified, a game abstraction).

## Items (data: `economy/items.rs`)
`Item` is one enum (serde, `Ord` for `BTreeMap` keys). Existing workbench materials become items: `Culture`, `Tooth`,
`Grit`, `Glass`, `Valve`, `Clam` (the larder still holds clams as today; a clam carried home goes to the larder).

- **Raw** (from nodes): Mussel, Clam, Scallop, SeepMussel, VentClam, Kelp, Quill, Glass, BlackCoral, BambooCoral, Grit,
  Clay, Tooth, WhaleBone, Nodule, Snail, Worm, Lanternfish, Grenadier, ScalyFoot, ShoreStrain, Culture,
  TwilightStrain, SulfurMat, VentMat.
- **By-products:** Valve (shucking any bivalve gives one; eating a clam gives two, as today), Pearl, Scale, Spoiled.
- **Processed:** KelpCord, BraidedCord, ShellBlank, QuillRod, SpiculeBundle, BoneSplint, NodulePlate, ScalePlate.
- Tools, gear and food portions are **not** items: they are equipped state (`econ.tools`, `econ.gear`) or portions
  (`PreparedFood`).

Each item has: `names() -> (one, many)`, `big() -> bool` (WhaleBone, Grenadier), `tier() -> u8`, `word()` (replay/code
word, kebab-case), and an icon key for `src/`.

The workbench `Stash` becomes `Stash(BTreeMap<Item, u32>)` with `count/add/has/take`; this is the **bank**: unlimited,
at home, kept through death. Carrying gets one new variant `Carry::Item(Item)` for every new item; the existing
`Carry` variants stay for the unique pickups (eggs, the pit culture, the shed tooth, the buried sponge stay personal
one-off progression finds, untouched).

## Tools and gear (data: `economy/gear.rs`)
Three tool kinds: **pry tip** (Prying), **scoop** (Excavating), **cutter** (Harvesting). One of each is fitted at a time.

| Tool tier | Material | Made from (Shellcraft at) | Shellcraft lvl | To use: skill lvl | Uses before cracked |
|---|---|---|---|---|---|
| 0 | bare claw | — | — | 1 | ∞ |
| 1 | shell | ShellBlank + KelpCord (workbench) | 1 | 1 | 60 |
| 2 | quill | QuillRod + KelpCord (workbench) | 15 | 15 | 90 |
| 3 | glass | SpiculeBundle + BraidedCord (grindstone) | 30 | 30 | 120 |
| 4 | bone | BoneSplint + BraidedCord (grindstone) | 45 | 45 | 160 |
| 5 | nodule | NodulePlate + BraidedCord (grindstone) | 60 | 60 | 220 |

**No soft lock (proved by a test):** tool tier k is made from tier-k node material, and tier-k nodes need tool tier k−1.
A cracked tool (0 uses left) works as tier 0 until **repaired** at the workbench: `Grit × tier + KelpCord × 1`, any tier,
restores all uses (the common-resource sink). Tools are never destroyed. A new crab starts with tier-0 everything; the
tutorial nudge (first time near a T2 node): "A shell pry tip would open these. Make one at the workbench."

**Gear** (visible on the crab; drawn by `src/`, placeholder sprites that read clearly; the rig hides unfitted parts):

| Gear | Slot | Made from | Lvl (skill) | Does |
|---|---|---|---|---|
| Kelp sack | bag | KelpCord × 4 | 8 Weaving | +2 shell slots |
| Woven pack | bag | BraidedCord × 3 + Quill × 2 (loom) | 35 Weaving | +4 shell slots |
| Coral-ribbed pack | bag | BraidedCord × 4 + BambooCoral × 2 (loom) | 62 Weaving | +6 shell slots |
| Shell plating | body | ShellBlank × 3 + KelpCord | 5 Shellcraft | −10% damage taken |
| Quill-ribbed plating | body | QuillRod × 3 + BraidedCord | 20 Shellcraft | −15% |
| Bone plating | body | BoneSplint × 3 + BraidedCord (grindstone) | 50 Shellcraft | −22% |
| Nodule plating | body | NodulePlate × 3 + BraidedCord (grindstone) | 65 Shellcraft | −28% |
| Scale mail | body | ScalePlate × 4 + BraidedCord (grindstone) | 80 Shellcraft | −35% |
| Black-coral claw tips | claws | BlackCoral × 2 + BoneSplint (grindstone) | 55 Shellcraft | +15% claw damage |
| Clam-shell guards (exists) | claws (block) | Valve × 2 | 1 | unchanged |

Plating cracks: each blow taken wears it by 1 (uses: 40 / 60 / 90 / 120 / 160); cracked plating gives nothing until
repaired (`Grit × tier + Valve × 1`). Shell slots = `carries()` (4 × size², as today) + bag.

## Crafting (data: `economy/recipes.rs`; runtime: `craft.rs`)
A recipe: `{ word, name, skill, level, station, needs: &[(Item, u32)], makes: Output, xp }`. Output is
`Items(&[(Item,u32)])`, `Tool(kind, tier)`, `Gear(gear)`, `Portions(kind, n)`, `Station(station)`, `Bed`, `Repair`.
The menu shows every recipe; unmet ones are greyed with the reason ("Cooking 25", "Needs a brine pot", "2 more clay").
Making is a command through the sim exactly like today's `Sim::make` (only its own crab, tucked in at home; replays note
`craft=<word>`).

**Stations** (Building; built at home from the Build tab):

| Station | Building lvl | Needs | Unlocks |
|---|---|---|---|
| Workbench | — | exists | T1–T2 shellcraft, cord, shucking |
| Culture bed 1 / 2 / 3 / 4 | 5 / 20 / 40 / 60 | Grit 3 + Valve 2 / Clay 3 + Grit 3 + Valve 2 / WhaleBone 1 + Clay 3 + mantis club 1 / Nodule 2 + Clay 4 + jelly mesoglea 2 | Culturing beds |
| Loom | 12 | Quill 4 + KelpCord 4 + Grit 3 | Braids, packs |
| Brine pot (clay curing pot) | 10 | Clay 3 + Valve 2 | Curing recipes |
| Grindstone | 18 | Grit 6 + Clay 2 + snapper claw 2 | T3+ tools, plating |
| Fermenting crock | 30 | Clay 6 + Glass 1 | Fermenting |
| Home stages (exists) | 1 / 3 / 5 grit built | Grit | unchanged; each grit built gives 20 Building XP |
| Whelk room / hall / keep | 30 / 45 / 62 | whelk shell 1 + Clay 4 / whelk shell 2 + Clay 6 + octopus ink 1 + braided cord 2 / whelk shell 3 + Nodule 2 + black coral 2 | home stage 1 / 2 / 3, in order |

Building XP per station = 40 × its level requirement / 5 (minimum 40).

**Processing chains** (raw → processed → finished), XP to the recipe's skill:

| Recipe | Skill lvl | Station | Needs | Makes | XP |
|---|---|---|---|---|---|
| Kelp cord | Weaving 1 | workbench | Kelp 2 | KelpCord 1 | 8 |
| Braided cord | Weaving 20 | loom | KelpCord 3 | BraidedCord 1 | 30 |
| Shell blank | Shellcraft 1 | workbench | Valve 2 + Grit 1 | ShellBlank 1 | 12 |
| Quill rod | Shellcraft 15 | workbench | Quill 2 | QuillRod 1 | 28 |
| Spicule bundle | Shellcraft 30 | grindstone | Glass 2 + KelpCord 1 | SpiculeBundle 1 | 55 |
| Bone splint | Shellcraft 45 | grindstone | WhaleBone 1 | BoneSplint 2 | 100 |
| Nodule plate | Shellcraft 60 | grindstone | Nodule 2 + Grit 1 | NodulePlate 1 | 170 |
| Scale plate | Shellcraft 70 | grindstone | Scale 3 or ScalyFoot 2 | ScalePlate 1 | 220 |
| Tools and gear | per tables above | | | | 2 × the inputs' summed tier XP |

**Cooking** (no fire). Foods become portions (`PreparedFood`, now 4 slots; portions cling to the shell and are eaten by
holding Eat, as today). Each batch makes 2 portions. Spoil chance (fermenting and curing only) =
`max(0, 30 − 2·(level − req))%` by `economy::roll`; a spoiled batch gives Spoiled × 1 (a culture-bed feed: halves its
grow time once) and half XP.

| Food | Cooking lvl | Station | Needs | Portion does (for 3 min unless instant) | XP |
|---|---|---|---|---|---|
| Shell food (exists) | 1 | workbench | Clam 1 (larder) | mends 20% shell (instant) | 12 |
| Reserve food (exists) | 1 | workbench | Clam 1 (larder) | 3 reserve strokes (instant) | 12 |
| Shucked mussels | 1 | workbench | Mussel 2 | mends 15% shell (instant); +1 Valve | 10 |
| Snail and kelp wrap | 5 | workbench | Snail 2 + Kelp 1 | +1 reserve stroke now, reserve refills 25% faster | 16 |
| Worm paste | 12 | brine pot | Worm 2 + Kelp 1 | gathering 10% faster | 30 |
| Clay-cured scallop | 28 | brine pot | Scallop 2 + Clay 1 | gathering 15% faster | 55 |
| Fermented lanternfish | 35 | crock | Lanternfish 2 + ShoreStrain 1 | pressure strain −30% | 70 |
| Grenadier ferment | 45 | crock | Grenadier 1 + SulfurMat 1 | claws +15% | 95 |
| Vent-poached seep mussels | 50 | **vent pot** | SeepMussel 2 (on the shell) | mends 40% shell, full reserve | 110 |
| Vent clam feast | 65 | **vent pot** | VentClam 2 + ScalyFoot 1 (on the shell) | all of the above, 4 min | 180 |

One food buff at a time (a new one replaces it). The **vent pot** is a station node in the vent field: hold Gather at it
with the ingredients on your shell; it cooks them there (the deep's reward is cooked on the spot).

**Culturing beds** (the farming analogue, at home, personal): plant one strain → it grows for its time, then harvest
gives `3 + level/20` of that strain and XP. Beds keep growing while the crab is out (sim ticks; not while the game is
closed).

| Strain | Culturing lvl | Grow time | Plant XP | Harvest XP | Used for |
|---|---|---|---|---|---|
| Shore strain | 1 | 2 min | 5 | 20 | Fermenting starter; a dim lantern top-up |
| Culture (pit glow) | 15 | 4 min | 12 | 50 | Brighter lantern (exists), crock fermenting |
| Twilight strain | 30 | 6 min | 25 | 100 | Lantern: +1 lantern step |
| Sulfur mat | 45 | 9 min | 45 | 180 | Grenadier ferment; bed feed |
| Vent mat | 60 | 12 min | 80 | 300 | Vent clam feast alternative starter; best bed feed |

**Survival** XP: per second in a band ≥ 2 (twilight): `band − 1` XP; ×2 while pressure strain is above 0.5.

## Creature drops (data: `economy/items.rs` `from_drop`, `economy/recipes.rs`; P9)
Eating a beaten creature's body (the same host-refereed take as before) also puts that creature's drops on the shell. No
room: they are left behind and the shell says it is full (`shell_full`). An amphipod swarm has a body like the rest, so
"scooping the dispersed swarm" is the same take. Item tier is the zone the creature lives in. Fighting XP for a beaten
bestiary creature is `max(15, 0.5 x vitals.health)` (squid 70, lobster 60, isopod 45, hagfish 30, hermit 25, bobbit 20,
viperfish 17.5); the older creatures keep the flat win bonus. A renamed `DROPS` string fails a test.

| Creature | Zone | Drop | Used for |
|---|---|---|---|
| Humboldt squid | twilight (3) | squid beak | Beak claw tips: 2 + braided cord, Shellcraft 35, grindstone, claws +10% |
| Viperfish | midnight (4) | viperfish fang | Fang claw tips: 2 + bone splint, Shellcraft 50, grindstone, claws +18% |
| Viperfish | midnight (4) | photophore | Photophore lantern: 1 + twilight strain, Culturing 40, +1 lantern step |
| Hermit crab | basin (2) | whelk shell | Whelk room: 1 + 4 clay, Building 30, home grows one stage |
| Hermit crab | basin (2) | anemone | Anemone guards: 1 + 2 valves, Shellcraft 20, clam-shell guards |
| Giant isopod | midnight (4) | isopod plate | Isopod plating: 3 + braided cord, Shellcraft 57, grindstone, -25%, 140 blows |
| Bobbit worm | basin (2) | bobbit jaw | Jaw claw tips: 2 + braided cord, Shellcraft 40, grindstone, claws +12% |
| American lobster | twilight (3) | crusher fragment | Crusher-fragment guards: 2 + kelp cord, Shellcraft 25, claws +8% |
| Hagfish | midnight (4) | slime thread | Slime cord: 2 + kelp cord, Weaving 45, loom, makes 3 braided cord |
| Amphipod swarm | midnight (4) | amphipods | Amphipod broth: 3 + kelp, Cooking 40, brine pot, pressure strain -20%, +1 reserve stroke |
| Turtle, grouper | | none | |

The original creatures drop too (the same take; Fighting XP stays the flat win bonus). Which creature lives where is the
level's; the item tier is the nearest zone. Real parts, but that crabs could use them is "not verified".

| Creature | Zone | Drop | Used for |
|---|---|---|---|
| Snapping shrimp | shallows (1) | snapper claw | Re-tip a worn tool: 1 claw + Grit x tier (5 recipes, one per tool tier, Shellcraft 1); grindstone (2) |
| Snapping shrimp | shallows (1) | shrimp tail meat | Snapper broth: 2 + kelp, Cooking 10 (mends 25%); shrimp-paste feast: 3 + 2 worms, Cooking 55, crock, mends 35% and gathers 15% faster |
| Peacock mantis shrimp | shallows (1) | mantis club | Club claw tips: 2 + kelp cord, Shellcraft 16, claws +9%; culture bed 3 |
| Snowflake moray | trench (2) | moray tooth | Mend a worn tool (1 + 2 grit, Shellcraft 1); moray-tooth claw tips: 3 + bone splint, Shellcraft 42, claws +14% |
| Queen triggerfish | basin (2) | triggerfish plate | Mend worn plating (1 + 2 grit, Shellcraft 1) |
| Alarm jelly | midwater (2) | jelly mesoglea | Jelly salad: 1 + kelp, Cooking 6; culture bed 4 |
| Common octopus | far rim (3) | octopus ink | Inked kelp: 1 + 2 kelp, Cooking 14 (+2 reserve strokes); whelk hall |

Every drop, old and new, also has a part of a high-level recipe or a repeatable sink (mending, eating, planting) **and**
a recipe below level 20 (`economy_every_way_pays.rs`). The older ten got the early uses: mending a worn tool or plating
with one part and 2 grit (beak, fang, bobbit jaw, moray tooth / crusher fragment, isopod plate, triggerfish plate, whelk
shell, anemone glue), slime twine (Weaving 10: 1 thread + 2 kelp cord makes a braided cord without a loom), a photophore
seed (Culturing 12: 1 photophore + 1 shore strain makes 2 glowing cultures, the way a squid's light organ keeps glowing
bacteria), and a plain amphipod broth (Cooking 12). A new Output, `Repair(AnyTool)` and `Repair(AnyPlating)`, mends the
first worn tool or the worn plating of any tier.

One claw gear at a time (the claw slot); tips replace each other. Zones are design readings, not verified.

## Every way to play pays (owner's three rules; data tests in `economy_every_way_pays.rs`, pacing in `economy_pacing.rs`)
**1. Two hours of low-attention gathering near home pays.**
- *Safe.* The tier 1 nodes on the home side lie in two pockets, 10 units apart: x 222 to 242 (after the home burrow's
  first silt shaft, whose mouth reaches x 220, and before the first snapping-shrimp burrow at 340, which notices within
  120 and strikes within 72) and x 432 to 492 (between the burrows at 340 and 590). Ten nodes, two of each skill (the
  first pocket holds Culturing, Harvesting and Foraging, the second all five and a second Prying and Excavating); a crab
  standing in a pocket reaches the whole of it (reach 70). Test: no foe of the level or the bestiary lies nearer a home
  node than its danger distance (a turret: strike range + 20; a roaming hunter: 600); no node lies in a door's mouth;
  and a bot working the real, populated level for 30 minutes is never hit. (#610 put the first shaft's mouth at x 124 to
  220, so the old first pocket at 175 to 247 lost seven of its nodes' places: there were twelve on foot, ten now.)
- *Close.* The farthest home node is 342 units from the door: 1.2 s on foot (test: at most 40 s). The shell's four slots
  are the only reason to stop; the kelp sack (Weaving 8, 8 kelp) stays where it is: it comes within the first quarter
  hour and the trip home is two seconds, so an earlier sack buys little.
- *Steady XP and stack, below an attentive dive* (bot on real ticks, 2 sim hours, watching only the full shell):

| Crab | Nodes worked | XP / h | Levels gained | Items banked / h |
|---|---|---|---|---|
| Level 1 | mussels only | 9,850 | 64 (summed) | 988 |
| Level 1 | all five skills | 13,575 | 92 (summed) | 1,418 |
| Level 60 | mussels only | 11,155 (186 / min) | 0 | 1,120 |
| Level 60 | all five skills | 19,571 | 0 | 2,046 |

  Tested: a level 1 crab on one kind reaches level 20 or more in 2 h (measured 33); a level 60 crab on one kind earns
  at least 25% of the tier 3 round trip at level 60 (700 / min on the Plate D map; measured 27%) and still less than it (tier 1 stays
  below tier 3 per minute). The pace is node-bound (2 mussel clumps on foot now, 5 charges each, 45 s regrowth).
- *Useful.* Every tier 1 item is an input to a repeatable sink or a recipe at level 40 or more (mussel and snail to
  food, kelp to cord, grit to every repair, shore strain to planting); data test.

**2. Farming creatures you have outgrown pays.** Every drop has a use below level 20 and a high-level use or a repeatable
sink at every level (table under "Creature drops"; data test over every drop of every kind). Snapping shrimp: claws to
re-tip a tool of any tier, tail meat to broth and a high-level feast.

**3. Building takes part in other activities.** Sources: Gathering (Prying, Harvesting, Foraging, Culturing items, valves),
Fighting (creature drops), Diving (items of tier 4 or more: deep nodes and deep creatures) and Digging (Excavating items).
An item may have several (a nodule is Digging and Diving; a processed item has those of its first recipe's inputs). Every
Building recipe above level 10 draws on at least two; no single skill or Fighting supplies them all; the set needs all four.

| Recipe | Level | Activities |
|---|---|---|
| Culture bed 1 | 5 | Gathering, Digging |
| Brine pot | 10 | Gathering, Digging |
| Loom | 12 | Gathering, Digging |
| Grindstone | 18 | Fighting, Digging |
| Culture bed 2 | 20 | Gathering, Digging |
| Fermenting crock | 30 | Gathering, Digging |
| Whelk room | 30 | Fighting, Digging |
| Culture bed 3 | 40 | Fighting, Diving, Digging |
| Whelk hall | 45 | Gathering, Fighting, Digging |
| Culture bed 4 | 60 | Fighting, Diving, Digging |
| Whelk keep | 62 | Gathering, Fighting, Diving, Digging |

## Danger guards the way, never the harvest (owner, 2026-11-11; P11)

Danger is on the way to a resource, never on the resource: once a crab reaches a node, harvesting is calm. A deep
gatherer can mostly avoid danger by careful routes, not fights.

What is enforced, and by which test (`economy_every_way_pays.rs`):
- **No foe reaches a node.** `no_foe_lies_near_any_node_on_the_real_level`: for every placed node on the real level,
  no foe from any spawn list (`level.foes`, which holds the older route, the bestiary, the exploration habitat, the
  canyon jellies and the octopus; `creatures::spots`; the moray crevices) lies within its danger radius, in two
  dimensions. The radius: a turret (snapper, moray, bobbit worm) its strike range + 20; the octopus its notice
  range + 20; a tethered walker (hermit crab, giant isopod, lobster) its leash + bite + 20 (measured: an isopod
  shoved a crab 400 from its burrow, so notice range alone is not enough); the hagfish its leash + ink cloud; any
  other hunter (mantis, midwater creatures, amphipods) 600. The sixgill is a band resident, not a spawn.
- **Nothing in the gather rules interrupts a harvest except the crab's own movement or a hit.** The cancel list in
  `economy/sim.rs` is walking, jumping, dodging, letting go, leaving the floor, a hit, a stagger; no foe, timer or
  event is on it. `a_harvest_at_every_node_is_calm_with_the_real_foes_alive` holds Gather for twelve seconds at all 82
  nodes with the real foes alive and fails on any hit or stagger. The bar emptying on each cancel is in
  `economy_gather.rs`.
- **Nodes placed on the Plate D map** (`economy/place.rs`, #572; foes never moved). A spot is an `x`, or it names a
  landmark (`Level::landmarks()`) and lies on the floor of that place; a level that lacks the place drops its nodes.
  The first basin (a moray in its crevice and a hermit crab leave no calm span) holds no node. Tier 1 is the home side's
  two pockets and the far ledge (x 1,850 to 2,470, clear of the snapper at 2,060 and the moray at 2,650). Tier 2:
  the **home burrow's clay gallery** (a tunnel, the fear register "safe") holds NO nodes (lead's call after P8: 23.5 s of dig from the door, tier 2 there paid 938 XP/min against the hub's 403; the clam beds and clay banks moved to the hub, x 4,527 to 4,802) — it was to hold the three clam beds and three clay banks
  (Prying and Excavating, x 268 to 468, 40 apart; the world's own two gallery clams lie at 330 and 440); the other three
  skills stay in the far basin hub (x 4,500 to 4,940, 55 apart). Tier 3: the doorstep (5,443 to 5,790) and the rim short
  of the octopus (7,905 to 8,015) keep the scallops, glass gardens and twilight glow; **the bone hall's floor** (x 3,880
  to 4,130, 50 apart, in sight of the door at 3,800 and the seep shaft) holds the fish falls and tooth beds as the
  chamber's prize. Tier 4: the canyon floor's far end (black coral, whale-fall bones, grenadier carcasses; the lobster
  holds the near end) and **the seep garden** (seep mussels and the sulfur mat, x 19,430 to 19,605, east of the door
  and out of the isopod's leash). Tier 5: the vent field's east end (x 19,730 to 20,450, both vent pots; west of the
  crust vault's door; the amphipods, a hagfish and an isopod hold the west end). Every band, rock, doorway, mouth and
  spacing rule still holds (`economy_core.rs`), and a crab stands on the real cell floor of each pocket node with the
  real foes alive for twelve seconds of gathering (`economy_every_way_pays.rs`).
  Cost: tier 2 is now two ways to farm (a 23.5 s dig round trip to the gallery, a 95 s walk to the hub); see Where
  things stand.

## Levels and gear: Elden Ring scaling (owner, 2026-11-11; P11)

Levels and gear lower the skill a fight needs but never remove it. More shell, harder hits and slightly wider windows
are fine; a fight never becomes click-and-see, and an under-levelled crab can still win on skill.

What is enforced (`economy_skill_scaling.rs`, caps in `economy/mod.rs`):
- **Damage taken:** plating and food together cut it by at most 40% (`MAX_DAMAGE_CUT_PERCENT`, factor never below 0.6).
  Today's best plating (scale mail) cuts 35%; no food cuts damage yet, the cap covers the sum when one does.
  `plating_and_food_never_cut_damage_below_sixty_percent`.
- **Damage given:** claw tips and food together hit at most 1.35 times as hard (`MAX_CLAW_FACTOR`; fang tips with the
  grenadier ferment would be 1.357 and are held at 1.35). The Fighting level's own +45% cap is separate.
  `claw_tips_and_food_never_hit_harder_than_one_point_three_five`.
- **Tells and windows are not the economy's:** no tell, telegraph, parry or dodge window is read from gear, food or
  levels. `tells_and_windows_do_not_depend_on_economy_gear_or_levels` shows the tuning is unchanged for a bare and a
  maxed crab and that the snapper (54-tick tell) and the mantis (78) tell the same ticks to both.
- **A level 1 crab with no gear can win on skill:** `a_level_one_crab_with_no_gear_still_beats_the_snapper_and_the_mantis_on_skill`
  plays a scripted crab (dodge the snapper's shock, parry the mantis's club, smash in the recovery or stagger) and
  ends both without losing health. Older scripted wins: the hermit crab and the lobster
  (`a_scripted_crab_beats_or_drives_it_off`), the isopod and the bobbit worm.

## Inventory pressure, sinks and pacing
- Shell slots `4 × size²` + bag (+2/+4/+6). Big items one at a time on the back. A full shell stops gathering ("No room
  on your shell"); the crab can drop items (existing Drop), which lie as its own recoverable cast-offs.
- Sinks: food (eaten), tool and plating repair (grit, cord, valves forever), culture bed feed, stations, home stages.
- Bank: the home stash, unlimited, kept through death. The larder keeps clams.

**Pacing targets** (enforced by tests in `crates/sim/tests/economy_pacing.rs`; a scripted bot plays real ticks near
home; deeper tiers combine measured gather times with route-solver travel times):
| Target | Bound |
|---|---|
| First level-up of any gathering skill from a new game | ≤ 90 s |
| Level 5 of one gathering skill, T1 only, round trips home | ≤ 6 min |
| Level 10 | ≤ 15 min |
| Level 15 (tier 2 opens) | ≤ 35 min |
| XP per minute (round trips, at the tier's unlock level, tier tool, best bag at that stage) | strictly rises T1 < T2 < T3 < T4 < T5 |
| Same at level 60 with nodule tools and the woven pack | strictly rises with tier |
| T1 "power-gathering" (dropping items, node-bound) XP/min | < T3 round-trip XP/min |
| Resources per minute vs distance | reported (falls with distance; value rises) |
| Every recipe output reachable from a new crab (fixed point over nodes, tools, stations) | no soft locks |
| Every node reachable from home by the route solver with the stage's kit | yes |
Reported (printed, not asserted): estimated hours to 30, 45, 60, 99.

## Interface (all in `src/`, phone first, follow `src/input.rs`)
- **Gather:** keyboard `C` (hold; E and F are smash and guard), controller left-stick click, touch: a contextual **Gather** button appears beside the
  action stick when a ready node is in reach (hold). `Controls::gather_held`. A ring fills around the node while held.
- **Node look:** each kind a small clear placeholder sprite (kelp fronds, mussel clump, grit heap...), tinted by band;
  a spent node draws bare (stubs, empty mud). Prompt text near the crab: "Hold C: pry mussels", or the reason it can't
  ("Prying 15", "Needs a shell pry tip", "No room on your shell").
- **Skills panel:** key `K`, touch: a "Skills" button on the pause/home sheet. A 3-column grid of 13 skills (icon,
  level, a thin progress bar); tap a skill for its RuneScape-style guide: every unlock by level, done ones ticked,
  plus "XP to next level". Works at 375 × 812.
- **Level-up juice:** the crab's claws lift for 0.8 s, a short warm ring pulses out from the carapace (crab-sized,
  never full-screen), `Cue::LevelUp` plays a rising two-note chime (placeholder synth), the HUD notice
  "Prying level 15!" + unlock line holds 4 s. Every 10th level and 99 get a longer chord.
- **XP drops:** a tiny "+25 Prying" floats up from the crab on each gather (RuneScape's XP drops), fades in 1 s.
- **Crafting menu:** tabs Make / Cook / Shellcraft / Weave / Build / Culture / Repair inside the home; each row shows
  needs with counts, the level and station, greyed with the reason when unmet.
- **Memory map:** the crab remembers nodes it has seen (within lantern reach), their kind and whether they were ready
  when last seen (`crates/sim/src/memory.rs` records; `src/memory.rs` draws small icons). No omniscient map.
- **Sound hooks:** `Cue::Gather { crab, skill }` (each work stroke), `Cue::Gathered`, `Cue::LevelUp`, `Cue::Crafted`,
  `Cue::ToolCracked`; placeholder sounds in `src/sound.rs` that read clearly.

## Co-op
- Gathering: the local crab's completed gather takes `Thing::Node(i)` through the existing referee
  (`shared.rs`: host takes and tells, a guest asks); a charge given twice is impossible. Regrowth is on the host's
  clock (`Msg::Regrown` generalised to a `Thing`). The gather bar runs on the crab's own device.
- Personal: everything on `SimCrab::econ`. Remote crabs' econ is never changed.
- A co-op test: two sims, same commands, same hash; two crabs gathering one node never get more than its charges.

## Packages and dependency graph
```
P0 spec (this page) ─┬─> P1 economy core ─┬─> P2 nodes + gather + co-op ─┬─> P6 memory icons
                     │                    ├─> P3 crafting chains          ├─> P5 progression sims
                     │                    └─> P4 HUD, panel, juice, menu  ┘   (needs P2, P3)
                     └─> P7 docs/decisions (any time)          P8 final placement on the new map (after world W3+)
```
| ID | Package | Owns (only these files) | Needs |
|---|---|---|---|
| P1 | Economy core: items, RS table, 13 skills, SkillBook, `Sim::earn`, LevelUp event + cue, Stash as items, `Carry::Item`, node and recipe **data tables**, placement on today's map, `Shared` node state, stub hooks (`econ.tick`, gather-speed and damage multipliers) | `crates/sim/src/economy/*` (new), `skills.rs`, `craft.rs` (Stash only), `physics.rs` (Carry, StepEvents), `world.rs` (SimCrab field, hooks), `progress.rs`, `tuning.rs`, `cues.rs`, `shared.rs` (state only), tests that break; `src/` only to keep it compiling | P0 |
| P2 | Nodes runtime: gather control (`Controls::gather_held`, replay word), hold timer, requirements, take via referee, charges/regrow, tool wear, rare rolls, vent pot hook, co-op message, excavating XP from real digging, Survival XP | `economy/gather.rs` (new), `shared.rs`, `net.rs` (one message change), `physics.rs` (Controls), `world.rs`, `replay*`, `strain.rs`, tests | P1 |
| P3 | Crafting chains: recipes runtime, stations, tools/gear make and repair, cooking with spoil, portions to 4 slots and new buffs, culture beds, building XP | `craft.rs`, `prepared_food.rs`, `economy/recipes.rs`, `economy/beds.rs` (new), `codes.rs`, tests | P1 |
| P4 | Presentation: gather input + touch button, node sprites and prompts, skills panel, level-up juice, XP drops, crafting tabs, gear placeholders, sound placeholders | `src/` (new `src/skills_panel.rs`, `src/nodes.rs`; `input.rs`, `interior.rs`, `stamina.rs`, `effects.rs`, `sound.rs`, `main.rs`, `shell_inventory.rs`) | P1 (gather visuals finish after P2) |
| P5 | Progression sims and pacing tests; tune the tables to hit the targets | `crates/sim/tests/economy_*.rs`, the tables' numbers | P2, P3 |
| P6 | Memory of nodes | `crates/sim/src/memory.rs`, `src/memory.rs` | P2 |
| P7 | Docs: decisions rows, world.md links, sources | `docs/` | — |
| P8 | Final placement on the world build's new map (anchors to landmarks: home burrow clam beds become T2 prying, bone hall T4 whale bones, seep garden T4 seep mussels, chimney hall T5) | `economy/place.rs` | world W3.3+ merged |

Never two packages on the same file at once: P2 and P3 both need `world.rs`/`craft.rs` hooks, so P1 lays every hook
P2/P3 need (`SimCrab::econ`, `econ.tick(dt)` called from `Sim::tick`, `Sim::gather_speed(id)`, `Sim::damage_taken_factor`)
and P3 never edits `world.rs`; P2 never edits `craft.rs`.

## Pick a want (data and rules: `economy/wants.rs`; H1 of #617)
The home asks "what do you want to make?" before it asks anything else (`core-loop.md`, beats 0 to 2).
- **`Sim::wants(id)`** lists every recipe the crab meets the level and station for, or is within 3 levels of (`soon`),
  whose inputs (bank plus what clings to the shell) are short by exactly one kind of thing, any count. Fewest missing
  first, then the higher recipe level; the `soon` ones last. A larder recipe (clam food) and a recipe with no inputs
  are never wants.
- **Where it comes from** (`Want::sources`) is read from data: the nodes that yield it, the creatures that drop it
  (`drops_of`), the recipe that makes it. A node source carries `remembered_at`, the nearest node of that kind the
  crab has *seen* (its node memory), or None: there is no omniscient map. `band` is the depth band that kind grows in.
- **`Sim::next_unlocks(id)`** names up to three skills within a tenth of their next level and what that level opens;
  `Sim::level_up_tasks` is the level-up text plus a "New task: make X" line for each recipe the level makes a want.
- **The pinned goal** is personal (`Economy::pinned`, saved in `Progress`): `pin_want(id, word)`, `unpin(id)`,
  `pinned_status(id)` (the want, and `have_all`). A replay writes `pin=<word>` / `pin=none`. When it becomes `have_all`
  while the crab is away from home, `StepEvents::goal_ready` is set for one tick (once; losing a part re-arms it; being
  home needs no word). Making the recipe unpins it.

## Home pieces (data: `economy/home.rs`; H1 of #617)
Pieces built into the home change it for everyone in that home's world. Eight, each built once per world:
| Piece | Building | Needs | Does |
|---|---|---|---|
| Kelp curtain | 3 | Kelp x22, Kelp cord x2 | a swaying curtain (looks) |
| Glass lamp | 8 | Glass x16, Kelp cord x2 | a warm glow (looks) |
| Shell mosaic | 15 | Valve x8, Grit x4, Pearl x1 | a pattern on the wall (looks) |
| Whelk door | 20 | Whelk shell x1, Clay x2 | a door for the doorway (looks) |
| Bone arch | 35 | Whale bone x2, Clay x3, Mantis club x2 | ribs over the home (looks) |
| Photophore string | 40 | Photophore x3, Slime thread x2 | cold light on the roof (looks) |
| Coral garden | 55 | Black coral x2, Bamboo coral x1, Clay x4 | beside the door (looks) |
| Nodule hearth | 70 | Nodule x4, Scale plate x1, Clay x4 | warm vent-stone, no fire: the shell mends in half the wait |
- Every piece gives the builder Building XP: `40 x level / 5`, at least 40. The builder pays from its **own** bank;
  the piece is shared. It is built tucked in at home (`can_build_piece` / `build_piece`; a replay writes
  `craft=<piece word>`). Every piece draws on at least two activities from Building 10 up (tested).
- **Shared state.** `Shared::home_pieces`, host-authoritative exactly like a taken clam: it is a `Thing::Piece`, so a guest's
  request is `Msg::Take`, the host builds it once and answers `Msg::Taken` to everyone, and the guest pays and earns only
  on that word (a second crab asking for the same piece is refused and pays nothing). `Msg::World` carries the set to
  a joining guest. `Sim::home_pieces()` is what `src/` draws.
- **Saved** with the world part of `Progress` (`Progress::home_pieces`, next to `clams`): in solo and on the host the
  crab's own world is its home. A guest visiting saves its own world's set, not the host's.

## Hidden rules (from the world build; they apply here)
- Only `underwater*`, `physics/consts.rs`, `level/coral_rise.rs` and the level's floor and rocks force the ~9-minute
  rebake; the probe digest test says "rebake" if terrain behaviour changes. This economy changes none of them.
- `Level` derives `PartialEq` and serde: no new fields; derive placement with a function.
- The route solver plays real ticks and is slow: tight stop predicates; heavy surveys `#[ignore]`d, run by hand.
- Level lists (clams, cultures...) are found by place in tests, never `.last()`.
- `crates/sim` forbids `unwrap()` outside tests.
- UI systems' text queries must be disjoint (Bevy B0001 panics at startup and the unit tests don't catch it): run the
  native game for 20 s before pushing UI changes.
- CI flake: `cell_save::native` on Linux (#482): rerun the failed job.

## Where things stand
- **L1 (#622), the loop measured:** `crates/sim/tests/loop_pacing.rs` plays three wants through the beat sheet
  (glass lamp 4.8 min, bone arch 2.1, kelp curtain 2.8 against 20). Pieces are wants now. Guardrail: the gap to 20 is
  the world's (distance, route choice, crossing, way home), never padded with costs, slow cuts or repeat trips; piece costs
  stay at about one good trip (lamp 4 glass + 1 cord, curtain 6 kelp + 2 cords, arch one mantis club). The table and
  the checklist lines are in
  `core-loop.md` ("Measured loop").
- **H1 (#617), sim side:** pick a want, the pinned goal and eight shared home pieces are in `crates/sim`
  (`economy/wants.rs`, `economy/home.rs`, tests in `home_wants.rs`); the board, the HUD line and drawing the pieces are
  presentation work in `src/`. See "Pick a want" and "Home pieces".
2026-11-11 (lead designer). Read this first if you are continuing without the lead.
- **Merged:** P0 spec (#573), P1 core + P2 gathering (#596).
- **In the big PR (branch `issue-567-economy-loop`):** P3 crafting chains, P4 presentation (skills panel `K`, level-up juice, XP drops,
  node sprites, gear looks, gather key `C` / left-stick click / touch GATHER button, fill ring, prompts and why-not text,
  crafting tabs Make/Cook/Shell/Weave/Build/Culture/Repair with a pager), P5 pacing sims, P6 node memory, P7 docs. It
  closes #567 to #571.
- **Measured pacing** (`crates/sim/tests/economy_pacing.rs`, a bot on real ticks, sim seconds): Prying L2 26 s, L5 144 s,
  L10 454 s, L15 945 s (all five gathering skills within the targets; Culturing slowest at L15 1,786 s; measured again
  after the nodes moved into the pockets). XP per minute
  on round trips by tier at unlock / at level 60: T1 203 / 489, T2 351 / 506, T3 500 / 540, T4 570 / 603, T5 624 / 629
  (strictly rising). Items per minute fall with distance: 20.3, 3.2, 2.5, 2.2, 0.8. T1 power-gathering 157 to 180 XP/min (3 mussel clumps on foot).
  Estimated hours to 30 / 45 / 60 / 99: 0.7 / 2.3 / 8.3 / 340 (0.6 / 1.9 / 6.2 / 213 on the Plate D map, with tier 3's
  XP at 160). Trips (route solver, full kit, seconds round trip): T2 95, T3 164, T4 187 (80 / 96 / 123 on the Plate D map);
  **T5 is measured (#631): 307 s** (`measure_vent_trip`: the solver to the lower canyon's floor at 151 s, a 1.5 s smash of the
  narrows lock, the walk to the first vent node at 167 s, then 21 s up the slump and 118 s from Coral Rise home; the solver has no
  smash move, and the deep-food key still read "available" on the way back, so the 21 s climb is the solver's, not checked against
  the tether by hand). The extrapolation (329 s) was close: tier 5 pays 1,049 XP / min at its unlock level and 1,063 at level 60,
  still strictly above tier 4, so its XP per gather is unchanged. The long chimney home (a 33 s free ride after act three) is not
  in the number.
- **Tuning that moved (P8, #572): tier 3 XP per gather 200 to 160.** The Plate D map put the tier 3 gardens 40% closer (a 164 s round trip became 96 s), which lifted tier 3 to 774 XP / min at level 30 and 876 at level 60 against 805 / 873 for tier 4, so XP / min no longer rose strictly and a level 60 crab on mussels fell under a quarter of tier 3. At 160: 620 / 700, tier 4 805 / 873, and mussels 27%. The ladder at the unlock level is now 180 / 403 / 620 / 805 / 985 (tiers 1 to 5), at level 60 423 / 587 / 700 / 873 / 998.
- **Tuning that moved from the first draft:** XP per gather by tier was 10 / 110 / 200 / 260 / 750 (was 10 / 25 / 50 /
  100 / 175; the old numbers made the basin worse than the shallows). Tier-1 nodes on the home side sit in two safe
  pockets at x 175 to 247 and 435 to 483 that a crab reaches on foot (past x about 600 the plateau needs a jump).
- **"Every way to play pays"** (owner's rules, PR #613 vision): drops for the original creatures, a use below level 20 and
  a high-level or repeatable use for every drop, Building from several activities (whelk room, hall, keep), home nodes in
  safe pockets, a 2 h low-attention measurement. See that section for the numbers. The pacing line above was
  re-measured after the move.
- **P9, P10, P11 in #601:** P9 creature drops (ten items, ten recipes, health-based Fighting XP); P10 every way to play
  pays (above); P11 the two newest owner rules: "Danger guards the way, never the harvest" (nodes moved clear of every
  foe, tested at all 82 nodes; tier 2 is the far basin hub only) and "Levels and gear: Elden Ring scaling" (damage cut
  capped at 40%, claw bonus at 1.35, windows untouched, a level 1 crab wins on skill).
- **Next, in order:**
  1. P8 (#572, **done in the loop-pacing PR #626**): nodes re-placed on the Plate D map by landmark name (see "Nodes
     placed on the Plate D map"). Findings: (a) the bone hall lies at about -2,500, the twilight's depth, so the band
     rule keeps tier 4 out of it: it holds tier 3 (fish falls and tooth beds), and whale-fall bones stay on the canyon
     floor; (b) the route solver walks and swims, it does not dig, so the pocket nodes (18) are "dug to" and left out of
     `TRIPS`; the tier 2 pacing row is still the hub's; (c) **the gallery undercuts the ladder**: a clam bed 23.5 s of dig
     from the door pays about 860 XP / min at level 15 against the hub's 351 and tier 3's 500 (printed by
     `experience_per_minute_rises...`, not asserted). Options for the lead, cheapest first: tier 2 clam beds in the
     gallery pay half XP; a regrowth long enough that the gallery's 3 + 3 nodes bind (they do not now: trip-bound); or
     only the clay banks in the gallery. (d) the first silt shaft's mouth (x 124 to 220) took the first tier 1 pocket's
     place: ten tier 1 nodes on foot, not twelve, and a level 1 crab's skills level about 1.5x slower (Culturing L10
     837 s of the 900 s bound).
  2. P9 (#602, **done in #601**): creature drops; see "Creature drops". The creatures lane owns `creatures/*`; the
     economy owns `economy/*` and `craft.rs`. Known gap: the hermit crab ends itself in its own `die`, so its kill
     pays no win XP (creatures lane).
  3. Excavating XP from real digging (**done, #631**): the cellular engine has no per-unit amount, so it is measured from
     outside. Every fourth tick a crab holding dig toward sediment (`game_dig_load` above zero) credits the 16-unit block under
     its claws; a block not among its last 96 earns 1 XP (`DIG_BLOCK_XP`), deterministic, local crab only, not saved. Digging in
     place or again in a refilled tunnel earns about nothing (4 XP back along a 60 XP tunnel; 1 XP for a minute in place).
     The home burrow's shaft and gallery pay 206 XP / min at 17.5 s end to end against 180 for a tier 1 round trip and 620 for
     tier 3 (asserted 0.5x to 1.5x of tier 1 and below tier 3, `economy_pacing.rs`).
  4. Look at the gather ring, prompt, touch button and crafting tabs in a browser at phone size (never seen running).
- **Hidden rules found:** node indexes come from `placed_nodes` (spots outside a small level are dropped), so never index
  `place::SPOTS` directly; nodes sit about 60 units apart with a 70-unit reach, so a spent node hands the gather to its
  neighbour; legacy clam beds fill the larder, and a full larder now banks clams as items; `StepEvents` must stay `Copy`
  (one `level_up` per tick, the last level crossed); the wall-clock test `headless_runs_far_faster_than_real_time` fails
  under heavy machine load (run it alone); a node's charges reach guests in `Msg::World`.

## Sources (this page)
None opened yet beyond those cited in `universe.md` and `world.md`; every biology claim above not in those is
"not verified".
