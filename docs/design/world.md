# World

You are small and the ocean is enormous. Each dive is an expedition you plan, pack for and survive.

- **Depth is the map.** Sunny, safe shallows; then the twilight zone, the midnight zone and the trenches, each
  darker, colder and stranger.
  Real ecology by depth, the food web and the rules every creature follows: `universe.md`.
- **Real limits:** oxygen (the oxygen minimum zone), cold, pressure (body chemistry, adapted to over time) and light
  (your lantern). You go deeper by preparing and adapting, not only by levelling.
- **The way home matters.** Getting back with what you found is half the challenge.
- **Home is a shell.** You start in a borrowed shell and grow it into a base: kitchen, workshop, beds for friends.

## The first loop (in the test level)
- **Home** is an empty sea-snail shell on the floor near the start, a warm glow in its doorway. Walk into the doorway
  and the crab goes inside: it backs into the opening, claws to the door, and disappears into the dark (crabs shelter in
  crevices, empty shells and litter; that they back in claws first: not verified). Tucked in, it rests fully (stamina
  bar and rings, and its shell mends after a moment unhit), and what it carried home is put by the workbench, for
  the player to make things from (below). Walk away (or jump or dodge) and it walks back out. Inside, the claws stay
  folded, and the view goes inside too (below).
- **Rest stops:** three crevices out in the level, a dark hollow under a slab of rock at the foot of a wall: one at the
  foot of the cliff's back where the crab drops into the basin, one at the bottom of the pit against its left wall,
  and one on the canyon's shelf (below).
  Deep-water crabs really use such places: large rock slabs and rubble give "many crabs small, safe habitats", rock
  ledges are good crab habitat, and *Bathynectes longispina* is "frequently observed hiding in depressions under
  exposed rocks" ([Wenner, SC DNR, "Ragged Claws Under Silent Seas: Deep Water Crabs of the Charleston
  Bump"](https://dnr.sc.gov//marine/sertc/CrabsPhotoDoc.pdf)). The crab tucks in the same way it does at home and
  rests fully (strokes, reserve, the shell mended after a moment unhit), but a crevice is not home: no workbench, no
  larder, no meal. The last place it rested, crevice or home, is where it wakes after dying ("A safe crevice. You'll
  wake here."). Each crab keeps its own, and it's saved. They sit at dead ends on purpose, so a crab walking past
  never backs into one by accident; past the trench every such wall foot is already a moray's.
- **Carrying: big finds on the back, small things on the shell.** A big find (the lantern culture, the shed tooth) rides
  on the crab's back, one at a time (sponge crabs really carry things on their backs). Small things (clams, shell grit)
  cling to its shell instead, up to 4 at once, drawn stuck on the carapace's dome so you see what you carry: decorator
  crabs really fix bits of their surroundings to their bodies, held mostly by hooked setae (hairs) on the carapace
  ([Lianos et al. 2022, "Much more than hooked: Setal adaptations for camouflage in *Macrocoeloma trispinosum*",
  Arthropod Structure & Development 66](https://pubmed.ncbi.nlm.nih.gov/34863064/)); that they carry food home this way:
  not verified. Neither blocks the other: a clam on the shell never stops the crab taking the culture, nor the
  culture a clam. With the shell full, a small thing stays where it lies ("No room on your shell"), never swapped.
  Tucked in at home, everything comes off at once (clams into the larder; grit, the culture, the tooth and the glass put by the
  workbench, to make things from). Four is enough that a small thing never costs a trip of its own (the shallows'
  two clams and heap fit, and so does the pit's clam and two heaps), and few enough that the larder can't be filled nor
  the home finished in one trip. Dying leaves all of it in the cast-off shell; walking back over it takes back
  whatever there's room for.
- **The lantern culture:** at the bottom of the pit, a patch of wild glowing bacteria (blue-green). Touching it puts a
  lump on the crab's back. Brought home, it's put by the workbench, and making a **brighter lantern** there
  cultures it into the lantern: the light reaches about a third further for each one. The trip home (climb out,
  cross the basin, the cliff
  steps, the trench) is the challenge.
- **Skills grow by doing (RuneScape):** each earns experience by doing it, and each level needs a little more
  than the last (level n+1 at n x (n+1) steps). A small row under the stamina bar shows each skill once it has any
  experience, and a level up says so ("Swimming level 2!"). They're saved with the crab, and only grow on its own
  device in co-op.
  - **Climbing:** experience for every bit climbed (0.01 a unit, up or down); level 2 at 20, 3 at 60, 4 at 120, 5
    at 200. Each level climbs a little faster (+8%). One climb out of the pit reaches level 3.
  - **Swimming:** 1 experience per paddle stroke (level 2 at 10 strokes, 3 at 30, 4 at 60, 5 at 100). Each level
    makes the stroke rhythm and the legs' rest on solid ground 5% shorter, up to level 10 (0.30 s and 0.6 s at
    level 1, 0.165 s and 0.33 s at 10). It never adds strokes or height: eggs still decide which routes are open.
  - **Fighting:** 0.25 experience per point of damage the claws deal a creature, 15 for beating one (a snapping
    shrimp is worth 18, the mantis 30, a moray 37.5); level 2 at 40 (two creatures), 3 at 120, 4 at 240, 5 at 400.
    Each level adds 5% to the claws' damage, up to level 10 (+45%).
  - **Data-Driven (the owner's above-water exception):** importing a workbook with at least three nonempty data
    rows and two named columns earns 40 XP once. A completed real tournament earns 10 + 1.5 per candidate, up to
    256 candidates per completion. Each observatory decision pays 20 XP once. Step 20 gives level 2 at 40 XP;
    rank, XP and completed work are saved. Cancelled/failed work pays nothing. Rank never changes model scores,
    physical combat power or biology. See `king-ddd.md` for the castle's activities and personal rewards.
- **Moulting and feats (`crates/sim/src/moult.rs`):** a physical skill at **level 5** makes its **feat** ready, a new move
  rather than bigger numbers: Climbing, **gripping dactyls**; Swimming, **broad paddles**; Fighting, **heavy
  crusher** (what each does: `how-it-plays.md`). But a crab's body only changes at a moult: growth and shape change
  happen when it sheds its old shell, and a fresh moult is soft ([CQ] in `art/crab/anatomy-notes.md`). The note
  says so ("Climbing level 5! A feat is ready: moult to grow it.") and the skill row reads "Climbing 5: moult" (then "growing" while soft, "feat" once grown).
  - **Proving moments: a feat is earned, not only ground out.** Long before level 5, one deed of the skill makes
    its feat ready at once, said with the same gold note and stinger: **Climbing**, one unbroken climb of 5000
    (up out of the pit, or the canyon's bare rock with glass grips; not the home kelp, which is 4870 to the
    light): "One long climb without letting go!"; **Swimming**, five paddle strokes in one swim touching nothing
    (a crab makes about one more stroke a swim than it has eggs, so it takes four of the six eggs, the hidden pit alcove's or the canyon shelf's among them): "A long swim
    touching nothing!"; **Fighting**, beating a creature with a parry's counter (the counter smash or the crush it
    clamps into): "Beaten with a counter!". Level 5 still makes it ready the long way. Each proof is the crab's
    own (`proven`, saved with it, kept through death, carried to friends in its updates). That deeds rather than
    size and hormones trigger a moult is a game rule, not biology (not verified as a mechanism).
  - **Getting ready:** be **well fed** (a clam eaten at home). The moult uses the meal up. (That a crab builds
    reserves before a moult: not verified.)
  - **Moulting:** tucked into the home shell, well fed, with a feat ready, the crab moults to grow **one** ready
    feat (one per moult; with several ready, "Next" picks), from the Moult row at the top of the workbench menu.
    The crab backs out of its old shell, which stays in the home: a **keepsake** on the shelf along the back wall,
    one per moult. Soft, the crab is drawn pale until the new shell hardens.
  - **Soft for a while:** 90 s of play out in the open, a third of that resting tucked in. Soft, the shell holds
    half as much, the claws hit at 60% and it eats nothing (eggs, clams, bodies: "Too soft to eat yet"). Going out
    early is allowed, at its own risk; dying soft, it wakes still soft. "Soft shell 1:12" under the shell bar
    counts down.
  - **Hardened:** "Your new shell has hardened: gripping dactyls!" The feat is its own. Feats, moults and a moult
    still hardening are saved with the crab.
  - **Growing:** each moult makes the crab bigger, at once, by less each time (a blue crab grows about a fifth a
    moult, less as crabs grow: [BM06], [SO02] in `art/crab/anatomy-notes.md`): 1.12, 1.21, 1.28, 1.33, capped at
    1.35. Bigger is tougher (shell 100 x size), stronger (blows x size), carries more on its shell (4 x size²),
    strides further and sinks faster (numbers: `how-it-plays.md`, Feats and moulting). The size comes from the
    moults made, so it needs nothing new saved or sent; a friend's screen draws it from their crab's updates.
  - **Tuned so** a normal run grows one feat around the middle and can reach all three by the end. Measured on the
    owner's own run (the replay in #162, played headless): about 12 minutes in, Swimming reaches 5 (the first feat);
    Climbing gets there after a climb or two out of the pit (it earned 165 by its first climb out); Fighting had
    265 by the end, so it needs the later fights (more creatures on the trips home, the octopus).
  - Per crab, as a sim command (`moult=climbing` in a replay, like a code); in co-op a friend's crab is never
    moulted on your device (a friend can stand guard while you're soft).
- **The light matters:** halfway down the pit's right wall is an alcove that looks like a solid lump of rock until a
  bright enough lantern comes near (the starting lantern doesn't reach; one brightened with the pit's culture does).
  Inside: a fourth egg and the first clue to the trench mystery, a shed tooth bigger than the crab (sharks shed and
  regrow teeth all their lives), hinting at the huge shape that passes in the pit: a bluntnose sixgill shark, far
  behind, rarely (the first dive, then at most once every five minutes), at its own depth and speed (`src/deep.rs`).
- **The tooth makes scissor claws:** the crab carries it home on its back (one big find at a time), and at the
  workbench in the shell the player can fit its edges along both fingers of the crusher claw (found things as gear):
  **scissor claws**. Higher up
  the pit's right wall, a passage is blocked by a **ghost net** (a lost fishing net): only a jump and four strokes from
  the alcove's roof reach it, and the scissor claws snap through it (gone for everyone once cut). Beyond: a short tunnel and a coral-lined shaft up to the far part of the basin, where a
  crack at the foot of the end wall shows **lights far below**. The crack is the way down, into the canyon (next
  section). High on the end wall's face, out in the water, a **coral** grows out of the rock (no single stroke from
  the floor reaches it), and a **sixth egg** drifts four strokes above its top: more than the legs have left on
  arriving, so only a crab with **gripping dactyls**, resting on the coral, swims on up to it.
  At the foot of the pillar, a lump of **brittle rock** (old coral rubble crusted together; that such rubble cracks
  under a heavy blow: not verified) seals a third patch of **glowing bacteria** in a hollow: only the **heavy
  crusher**'s full charge cracks it (gone for everyone, and saved).
- **Food (the "Prepare" step):** clams lie half-buried in the floor: two in the shallows near home, one in the basin
  below the cliff, and harder ones on the flats past the trench (by the moray) and at the bottom of the pit; three more in the canyon
  (two on its floor, one on the far rim). Walking
  over one sticks it to the crab's shell (with room for a few small things, see Carrying); its bed grows a new one
  after 3 minutes of play. There's no fire underwater, so food is prepared, not cooked: brought home, a clam goes
  into the **larder** (a small pile inside the doorway, up to 5; a clam with no room stays on the shell). Settled in at home and not already well fed, the
  crab cracks one open with its crusher and eats it (that crabs open clams by crushing or chipping the shell: not
  verified) and is **well fed** for 4 minutes of play: its shell holds 25% more and its reserve 3 strokes more, both
  filled. It ends all at once (the extra shell and reserve go, keeping what fits). Dying ends it too (it wakes
  hungry, as in Valheim), but the larder is at home and stays; a clam on the shell drops into the cast-off shell like
  anything carried. Each clam eaten leaves its two empty **valves** (the two halves of its shell, hence "bivalve")
  by the workbench, for clam-shell guards (below). Eating stays automatic.
- **Building (the "Build" step): the home grows with shell grit.** Small heaps of shell grit (pale pieces of old,
  broken shells) lie on the floor, more of them deeper: one in the shallows by the first snapping shrimp, one on the
  slope past the trench, one in the basin below the cliff, one in the far basin past the ghost net, two at the
  bottom of the pit and three in the canyon (one on its shelf, two on its floor). Walking over one sticks a piece to the crab's shell (a small thing, like the clam); each heap
  is there once. Brought home, it's put by the workbench, and making **build into the home** there builds a piece
  into the shell. Animals really do this: octopuses dig their
  dens into beds of discarded shells, which "line and stabilise the shaft-like dens", and arrange shells round the
  den's edge ([Hing and Godfrey-Smith, The Conversation, "Did they mean to do that? Accident and intent in an
  octopus's garden"](https://theconversation.com/did-they-mean-to-do-that-accident-and-intent-in-an-octopuses-garden-85462));
  that a crab builds its shelter this way: not verified. The home grows in three stages, each one felt there:
  - **1 piece, the doorway shored up** (grit packed round the rim of the opening): tucked in at home, the shell
    mends after 0.5 s unhit instead of 2 s.
  - **3 pieces, a grit shelf inside** (and grit heaped along the shell's back): the larder holds 8 clams, not 5.
  - **5 pieces, the lip grown out** into a broad, pale flare (a snail's shell grows at its opening: not verified),
    with grit crusted along the top: a meal eaten at home keeps the crab well fed for 6 minutes of play, not 4
    (Valheim's comfort, which makes resting last longer).
  The first stage is a stroll from home; the last needs the basin and the pit. A piece on the shell drops into the
  cast-off shell like anything carried; what is built stays built. Each crab grows its own home (in co-op each
  player's home is in their own game), and it's saved.
- **Inside the home: the workbench.** Tucked right in at home, the view goes inside: a cut-away of the snail shell
  (its wall, the coil of the whorl at the back, the doorway open to the sea on the right) with the crab resting in
  it, the larder's clams piled at the left, the grit built in so far (round the rim, the shelf, the crust), a hearth
  glow, and the **workbench**: a flat stone slab on pebbles, with what the crab has brought home lying on it (the
  tooth, glowing cultures, grit, a glass sponge, clam valves). Beside it, a menu lists each thing the workbench can make: its name,
  what it needs (have / need, green when there's enough), what it does, and a **Make** button (dim when it can't be
  made; gear already fitted says "Fitted"). The fight buttons hide while inside; walking out, jumping or dodging
  leaves as before. Making something is a command to the sim like the controls (a replay carries it as
  `craft=lantern`), only for your own crab, only tucked in at home, and saved. What comes home waits by the
  workbench until it's used: nothing is spent without the player choosing to. The recipes (a table in
  `crates/sim/src/craft.rs`):

  | Make | Needs | Does |
  |---|---|---|
  | Scissor claws | 1 shark tooth | Fitted to the crusher: snips through ghost nets (once) |
  | Glass grips | 1 glass sponge | Fitted to the leg tips: grip and climb bare rock (once) |
  | Brighter lantern | 1 glowing culture | The lantern reaches about a third further (each time) |
  | Build into the home | 1 shell grit | The home grows (stages at 1, 3 and 5 built, above) |
  | Clam-shell guards | 2 clam valves | Fitted to the claws: a block lets through half the damage it would (once) |

  **Clam-shell guards** are the new one: two empty clam valves lashed over the claws as shields. A blocked blow
  normally lets 20% of its damage through (40% once the guard is cracked); with the guards, 10% (20%). One clam eaten
  gives exactly enough, so the first meal at home already offers something to make. That a crab would armour its
  claws with clam shells: not verified (the game's idea).
- **What's fitted shows on the crab** (in the world and inside the home, both art styles): a cream ribbed clam valve
  over each palm (guards), the tooth's edges along the crusher's fingers (scissors), a pale icy glass sheath on each
  walking leg's tip (grips), and a jar with a sea-green speck per culture grown in (up to four) round a bigger glowing
  middle (the lantern). Grown feats show too: a crusher hand a quarter bigger (heavy crusher), broader blades on the
  paddles (broad paddles) and dark hooks curling in at the leg tips (gripping dactyls). They are rig parts in
  `crates/crabrig`, hidden until the sim says the crab has them.
- Eggs, cultures, skills, gear, cast-off shells, food (the larder, the beds growing back, the time left well fed)
  and the home (the shell grit taken and built in)
  are kept across reloads ("Start over" wipes them). A save from before the level grew keeps what it had; the new
  things are untouched (the crab's map memory starts over once).

## The canyon (below the crack)
The far basin ends at the edge of the shelf, where a **submarine canyon** cuts down into the twilight. Real canyons
are carved into the continental shelf and slope, and the tide runs along them: in Taiwan's Kaoping Canyon "the major
axis of the tidal currents aligned with the orientation of the canyon, and currents moved up-canyon during flood and
down-canyon during ebb", strongest towards the canyon's head [KPC08]; in Monterey Canyon tidal currents carry over 90%
of the flow's energy, with pulses along the floor over 60 cm/s [MC10]. Cold-water coral grows on such walls
(`universe.md`, Twilight Reef).
- **The crack** is a slot at the foot of the far basin's end wall, just wide enough for a crab: it drops down a
  chimney through the rock and walks out under the wall onto **the doorstep**, a ledge high on the canyon's near
  wall. Coral grows up the chimney (the way back out). The end wall is far too tall to swim over.
- **The shelf**, a sheer drop below the doorstep: cold-water coral climbs from it back up to the doorstep, a heap of
  shell grit, and a **rest crevice** at its back, against the wall (a dead end, like the others).
- **The canyon's floor**, a far longer drop below: a meadow of **sea pens** that glow blue where something brushes
  them. These are **the lights** seen from the crack. Real: MBARI found all nine deep-sea corals it studied make
  light, sea pens among them; *Funiculina* glows blue "when the stalk or polyps were disturbed" and a bamboo coral
  sent "brilliant waves of blue light that moved along the stem" [MBARI-GC]. Two clams and two heaps of grit lie
  there. Getting there ends the demo for now: a moment later a card says so ("The lights were alive."), lists what
  the crab found (eggs, strokes, lantern, scissor claws, skills), and offers "Keep exploring" or the report form.
- **The tide** runs up and down the canyon's middle (from its floor to well above the doorstep; still water along
  each wall). Squeezed from hours to a minute a round: the water runs **up for 24 s**, is slack 6 s, runs **down for
  16 s**, slack 6 s, each change a 2 s ramp. In it, the crab's drag is against the water, so the flood carries it
  up (it rises about 450 a second with no strokes at all; at the top it bobs until the tide turns) and the ebb drops
  it more than twice as fast as sinking (shelf to floor in 8 s instead of 18), too strong to swim up against. Marine
  snow streams with the water, which is how you see which way it runs; in the canyon a line under the depth counter
  says it too ("The water: running up", "running down", "still" or "turning"), a note says when it turns, and the
  first time the crab comes out into the canyon a note says the water moves here. **Nothing climbs from the canyon's floor:** the water is the way up. Ride the flood and steer off at the
  right moment onto the shelf, the doorstep or the far rim.
- **The upwelling: never stuck.** Along the near wall, from the floor to a little above the shelf, a lane of water
  runs up all the time, as strong as the flood, whatever the tide does: hop into it and it carries the crab up beside
  the shelf, which it steps onto; the shelf's coral climbs on to the doorstep. Real canyons carry upwelling, deep water
  running up-canyon onto the shelf while the wind drives it ("cross-isobath flow includes both upwelling and
  downwelling", driven by wind-driven shelf currents [ADM09]); here it never stops, so a crab on the floor can always
  get out (about 30 s to the far basin, a test checks from five spots at five points in the tide's round). The flood
  alone still reaches the fifth egg and the far rim. The note on arriving says it ("By the near wall it always runs
  up."), and so does the line under the depth counter there.
- **The glass sponge makes glass grips (the canyon's craft).** Among the sea pens near the far wall lies the
  skeleton of a dead **glass sponge**: a pale lattice of silica. Real: glass sponges are "common only in the deep
  ocean", their skeleton is built of silica pieces called spicules, and it "will often remain intact even after the
  sponge itself dies" [NOAA-GS]; *Euplectella*'s spicules even carry light like optical fibres [AIZ03]. The crab
  carries it home on its back (one big find at a time) and it's put by the workbench, where the player can fit its
  glass points to the leg tips: **glass grips** (that glass points hold a crab on rock: a design call, not a claim). With them the crab grips and
  climbs **bare rock**: pale, pitted bands down a few walls that its own leg tips slide off (kelp and coral climb the
  same either way). Two bands in the canyon:
  - **the near wall from the canyon's floor up to the shelf:** the way up without waiting for the flood (slow, free);
  - **the end wall's canyon face from the doorstep up to a notch** high above (2,060 above the doorstep; the flood's
    top and every stroke fall more than 300 short): in it, a second patch of **wild glowing bacteria**, a second
    culture for the lantern. Nothing else reaches it.
  It's the workbench's "Glass grips" recipe (1 glass sponge, made once; `crates/sim/src/craft.rs`). Fitted grips are
  kept through death and saved; tests: `crates/sim/tests/glass.rs`.
- **The far wall's cave**, cut back into the canyon's far wall in the still water beside the tide (x 7700 to 7901,
  y -5200 to -5030): the **fifth egg** drifts inside. The wall's face stands flush over the floor, and the cave is
  drawn as rock until a bright lantern comes near, like the pit's alcove, so nothing sticks out to give it away
  (players, #304, #305). Only riding the flood and pushing off it at the right height reaches it (four strokes from
  the floor in still water never do).
- **The far rim**, at the top of the far wall (the canyon's shallowest end, representing the ~200 m twilight boundary,
  not a literal measurement of this compressed map): a clam, and
  at its end, under the rock, **an octopus's den** (see below).
- Each thing in it is checked headless in `crates/sim/tests/canyon.rs` (reached with the strokes and the tide there
  are, and not without them).

### Spots left for creatures (coordinates in world units, `Level::test_level`)
- **Common octopus den** (`Level::den`): a dead-end chamber under the rock at the end of the far rim, inside
  x 8350–8800, y -1900 (floor) to -1620 (roof), the level's wall at its back; its floor a midden of shells (drawn,
  `src/canyon.rs`). Its boss lives there: the common octopus (`creatures/common-octopus.md`), hidden as a rock at
  x 8650 with its back to the wall, facing the way in. Beaten, it jets off and the midden is shell grit for the home.
  The common octopus lives above
  ~200 m, so its den is at the canyon's top, not its floor.
- **Alarm jelly** (`Level::drifting`): open water out of the tide, deep in the canyon: (6305, -6200) by the near
  wall and (7560, -7400) by the far wall, each at least 150 from rock and 100 from the tide.
- The tide's column is x 6450–7350, y -9400 to -700 (`Level::currents`): anything drifting in it should move with
  `Level::water_at`.

## The crossroads (the far basin as a hub)
The far basin, past the ghost net, is the hub of the deep part of the demo. Every branch out of it ends in a find
that changes how the crab moves or makes the trip shorter, and gives it a cheaper way home: every descent is a debt
(`vision.md`), and the branches are how you pay it off. The layout is in `crates/sim/src/level/hub.rs`; the promises
are checked headless in `crates/sim/tests/hub.rs` (reachability and softlocks), `dig_gates.rs` and `coop.rs`.
- **The ridge.** The pit's right wall stands on up as a ridge as tall as the end wall, so the hub is walled on both
  sides. Before it, a crab that walked off the cliff's crest glided right over the pit and landed in the far basin,
  skipping the ghost net (the scissor claws' gate); the ridge catches that glide, and the crab sinks into the pit.
- **The window (the way back in).** Through the ridge, where that glide passes, a window is closed by a second
  ghost net hauled tight from the hub side: its knots are only reachable from there (a level-design call, not a
  biology claim). A crab with scissor claws walking off the hub's floor into the shaft cuts it. From then on, a jump
  and a stroke off the cliff's crest carry a crab over the pit, through the window and onto the hub's floor: the
  trip from home to the hub skips the pit for good, and for everyone in co-op (it is a shared net). A crab that
  glides into the window before it is cut stands on its sill against the net and walks back off into the pit.
- **Glass grips: the long glide home.** Above the top of the shaft's coral, the ridge's hub face is bare rock. With
  glass grips a crab climbs it, free, to the crest (3,000 up, 3,700 above the hub's floor), and walks off west: the
  glide carries it over the pit, the basin and the cliff, and it lands by the trench, a short walk from home.
  Height is the credit against the descent's debt.
- **The tide: up the crack.** The flood runs up the crack as well as the canyon (tidal currents are strongest
  towards a canyon's head [KPC08]; that the crack carries it is a design call): jump in at the chimney's foot on the
  flood and the water lifts the crab to the hub, no climbing. On the ebb it runs down (a quick way into the canyon)
  and the coral is the way up, as before. Going down the crack on the flood means climbing down its coral.
- **Digging: the buried lattice.** A dead glass sponge's skeleton lies buried in the hub's sand, 70 under the floor
  near the shaft: a crab that digs there turns it up, and glass grips no longer need the canyon's floor. That a
  sponge skeleton ends up buried in sediment: not verified (the skeleton lasting after death is [NOAA-GS]). The
  ground has no tell drawn for it yet (`digging.md` asks for one).
- **The octopus** guards the last branch: the far rim, its den, and the reef beyond (next section).
- Not done (needs the cellular engine): the digging branch has no dug *tunnel* shortcut. The authored strata are
  a 100-unit band under each floor, so a tunnel that saves a real trip needs thicker strata in the terrain
  generator (`underwater.rs`).

## Eastern reef routes

Past the canyon rim, Rubble Banks leads down through Silt Hollow and back up toward Coral Rise.
The seabed remains the continuous route, with clam beds and resident predators. Two rest
crevices sit off that route: one against the rubble cap, one on the raised gallery ledge.
From the coral at x11100, an upper branch links coral crowns and three existing shelves to the
Silt Hollow gallery roof, then rejoins the seabed beside Coral Rise at x14900. The gaps and heights
vary: climb, kick toward the next crown, paddle if needed, then grip it. Landing on the middle
shelf offers the existing glass-sponge skeleton for crafting grips at home; passing above it
leaves the back free for the optional culture inside the gallery. The lower beds trade more
encounters for food; the upper branch trades climbing and paddle control for a quieter crossing.

Four eggs acquired before the eastern expedition are sufficient in the live-cell simulations,
including at maximum body size; the route does not require a feat, meal or glass grips. Both
directions work; stop on the broad upper shelf to refill paddle strokes before the final
uphill transfers. The gallery mouth returns to the lower seabed, where the crab can simply
walk back west without accidentally entering a haven. For a deliberate rest, enter a niche
to the left and leave it to the right. Both haven indices stay unchanged.

These are authored local reef shapes, not measured coral sizes or a claim about species
co-occurrence. Biology and resource behavior reuse the existing coral and glass-sponge design
assumptions above; no new biological claim or species is introduced.

## Zones and economy
Like RuneScape, rarer resources come from rarer, more dangerous places: depth is wealth.

| Zone | Example resources | Rarity | What they make |
|---|---|---|---|
| Sunlit Shallows | Kelp, driftwood, common shells, beach litter | Everywhere | Starter tools, basic stew, your first shell-home |
| Twilight Reef | Coral rubble, luminous bacteria cultures | Uncommon | Lanterns, reef armour |
| Midnight Zone | Whale bone, the village's trade | Rare | Strong claws, deep-adaptation food, trade goods |
| The Trench | Manganese nodules (abyssal plain), unknown things | Very rare | The best gear, and things nobody has made yet |

(Each must really exist where we put it; sources and the rules of the deep are in `universe.md`.)

- **Risk is the price.** Rare things are only worth something if you get them home.
- **Things get used up.** Food is eaten, lantern cultures die without care, gear cracks and needs repair, so common
  resources stay useful forever.
- **Trading is natural, never required.** Friends can specialise; solo players make everything (Ironman-style).
- **One world per group, for now.** Each P2P group's economy is its own; cheating only affects that group.

## Decisions so far
| Question | Call | Why |
|---|---|---|
| Camera | **Side-on 2D** (Silksong) with layered parallax | Going down feels like going down; suits hand-drawn art and tight movement |
| Look | **Hand-drawn ink + painted fills, oil-painted assets**, per-shape variation | The owner's style; fits phones; art can be made in code |
| Why dive | **To grow:** a bigger, stronger shell-home needs materials from deeper. Plus a quiet mystery: lights far down in the trench, stories at the whale-fall village | Ties economy, building and depth together |
| Surface | **A small beach and tide-pool hub at the top;** home sits in the sunlit shallows just below | A Valheim-style base; daylight is the safest thing in the game |
| Death | **Elden Ring respawns, Valheim corpses** (owner, 2026-11-11). You wake at the nearest rest point you have reached: home, a crevice rested in, or a region's entrance (passed within 120 units), never only at home. **Big fights** get a rest point at their door: die to a boss and you try again straight away, as you were, with no trek and no corpse run. **Overworld mistakes** leave your **cast-off shell** (a moult) holding what you carried where you died, to dive back for; worn gear stays with you. **Several cast-offs can wait at once**, Valheim style: die again before recovering one and both stay | Brutal but fair: you lose time and risk, not hours. Boss retries are instant; mistakes in the open create rescue dives |
| Neighbours | **A few characterful ones.** Crabs are rare; the whale-fall village is a misfit town of isopods, shrimp, an octopus trader and others | Warmth in the dark; few, so each matters |
| How surreal | **Real biology, no exaggeration.** No spells, no gods; ancient giants stay unexplained | The real ocean is stranger than anything we could invent |

A hardcore option (lose everything on death) can come later.

## Sources (this page)
- [KPC08] Wang, Lee & Liu (2008), "Observation of internal tidal currents in the Kaoping Canyon off southwestern
  Taiwan", *Estuarine, Coastal and Shelf Science* 80:153–160 (abstract, opened). <https://doi.org/10.1016/j.ecss.2008.07.016>
- [MC10] USGS, "Currents in Monterey Submarine Canyon" (abstract, opened). <https://pubs.usgs.gov/publication/70034693>
- [ADM09] Allen & Durrieu de Madron (2009), "A review of the role of submarine canyons in deep-ocean exchange with
  the shelf", Ocean Science 5, 607-620 (abstract, opened). <https://os.copernicus.org/articles/5/607/2009/>
- [NOAA-GS] NOAA Ocean Exploration, "What are glass sponges?" (opened). <https://oceanexplorer.noaa.gov/facts/glass-sponges.html>
- [AIZ03] Aizenberg, Sundar, Yablon, Weaver & Chen (2003), "Fibre-optical features of a glass sponge", *Nature*
  424:899–900 (via search results and a PNAS follow-up's summary, not opened: not verified).
- [MBARI-GC] MBARI, "Glow-in-the-dark corals light up the deep sea" (2020; Bessho-Uehara, Francis & Haddock 2020,
  *Marine Biology* 167:144). <https://www.mbari.org/glowing-corals/>


## Settled terrain on first arrival

Owner directive (2026-11-04): a newly discovered place must already look like an
established ocean floor. The opening's five-minute simulation bake is the model;
first arrival must not trigger a conspicuous initial sand collapse. Generate or
precompute deterministic settled starting material before it becomes visible,
without doing minutes of synchronous simulation at a cell-window boundary.

Settled means an initial condition, not frozen scenery. Currents, animals, digging,
and later disturbances must still move material, including the existing bounded
transport of falling sediment beyond the active window. Saved, edited terrain is
authoritative: this rule does not reset visited chunks or silently rebuild saves.
It does not promise a complete fluid simulation of every dormant region.

Boundary work must have a repeatable regression guard. Measure representative
fresh entries and archived returns, including paged simulation work, separately
from opening FPS. A 10 ms CPU budget is a useful calibrated target, not a universal
claim about storage, GPU latency, all phones, or an overloaded CI runner. The
`cell_transition_budget` example reports median/p95/max and accepts
`CRAB_TRANSITION_BUDGET_MS` for an explicit environment-specific clone-plus-remap p95 gate. Full paged and
ordinary material ticks are reported separately; ordinary
unit tests also protect material equivalence and bounded per-cell generation work.

### Transition measurement (2026-11-04)

On the development ARM64 Mac, 30 samples per case, actual workspace release
settings (`opt-level=z`, LTO, one codegen unit, `libm=3`), the column-local rock
sampler reduced fresh full-paged-tick medians as follows. The unchanged-main
comparison uses `69e0730` simulation code and the same benchmark/release settings.

| Crossing | Main median | Column sampler median | Column sampler p95 |
| --- | ---: | ---: | ---: |
| Opening east | 9.431 ms | 6.360 ms | 6.685 ms |
| Silt east | 13.427 ms | 7.342 ms | 7.830 ms |
| Canyon down | 18.201 ms | 9.774 ms | 11.089 ms |

The explicit 10 ms **clone + remap** p95 gate passed all six fresh/archived cases
(worst 4.164 ms); the full canyon tick did **not** meet 10 ms at p95. Full ticks
also include ordinary material physics (canyon ordinary material p95 3.157 ms),
page bookkeeping and simulation work. Disk requests are fulfilled as in-memory
missing-page replies; this is not an IndexedDB, rendering, network or phone
latency guarantee. Archived return timings remained approximately unchanged.

Run on an otherwise idle calibrated machine:

```sh
CRAB_TRANSITION_BUDGET_MS=10 cargo run -p crabsim --release --example cell_transition_budget
```

Without the environment variable it reports measurements without an uncalibrated
wall-clock failure. CI's deterministic structural test adds 1,000 distant rocks
and verifies the per-cell column list and resulting material remain unchanged;
the equivalence test checks floor bands and all authored rectangle edges.
The sampler optimization above intentionally preserved the original initial
materials. The subsequent [settled initial terrain](settled-terrain.md) work uses
an offline global material bake, with saved cells retaining precedence.

**Shelter in the open water.** Seven short clumps of deep-sea coral (200 units each) grow on the canyon's walls
(`crates/sim/src/level/refuges.rs`): from anywhere in the canyon's water one is in sight within 1,500 units. They
are spaced by more than a crab's highest jump with every stroke, so none is a step up to the next, to the shelf, the
far rim or the far-wall cave: the tide, the upwelling and glass grips stay the ways up. One sits just below the
far-wall cave's mouth (x 7670, y -5350 to -5150): the dead-end cave's retreat. Their places are a design choice.

## Bands

`Level::bands()` exposes immutable sim data in descending world y. `band_at` uses
only depth, with each border belonging to the deeper bed. The trench floor is
-260, the pit lip -700, the crack mouth -560 and its doorstep -1500, and the canyon
shelf -3400. Twilight begins at the pit lip so it covers the entire descent to the
shelf, including the crack. Deep starts at -9500, below the canyon floor at -9400,
reserved for W1.3. The first and last beds extend without limit.

| Name | World y range | Ambient light | Lantern reach factor | Sound activity | Grain hint | Smell | Intended residents |
|---|---|---|---|---|---|---|---|
| Shallows | y > -260 | 1.00 | 1.00 | 1.00 | Silt | Weed | Snapping shrimp, clams |
| Basin and mudbanks | -700 < y ≤ -260 | 0.45 | 0.95 | 0.70 | Silt over clay | Clay | Mantis, moray |
| Canyon walls / twilight | -3400 < y ≤ -700 | 0.12 | 0.82 | 0.40 | Clay | Clay | Lanternfish, octopus, alarm jelly |
| Canyon floor / midnight | -9500 < y ≤ -3400 | 0.03 | 0.65 | 0.12 | Pebbles | Sulfur | Whale-fall scavengers, sixgill |
| Deep | y ≤ -9500 | 0.00 | 0.55 | 0.04 | Crust | Metallic | Seep life, guardians |

Smells, grain hints and resident ranges are design choices, biology not verified.
Resident lists include planned animals and leave placement to their own packages.
Sound activity is data for W2.2. Grain hints describe future beds without changing
the settled terrain. The basin's primary grain hint remains silt.

Lantern drawing and memory both multiply their existing culture-dependent reach
by the bed factor sampled at the crab's eyes. The jar and halo shift from warm to
blue through sRGB multipliers, with identity tint in the shallows. Smell wisps use
the observer's bed and follow its reduced lantern reach. Sprite daylight uses the
ambient-light factor. Terrain retains its existing per-shape daylight curve and
receives the reduced lantern radius through its existing light uniforms. Opening
values remain identical. The depth gauge retains its existing long tail.

## Lower canyon (W1.3b / #487, pending seed bake)

The extension begins beyond Coral Rise's actual outer bank at x=17,600. The
existing floor is unchanged through that bank (including the issue's x=17,400
boundary). A small sediment heap rises eighty units, has an eighty-unit crest,
then drops 7,480 units at x=17,780–17,781 to y=-10,000. Water drag limits the fall;
falling does not damage the crab. The full kit cannot climb the face back up.

The narrows at x=18,000–18,080 is an eighty-unit-thick brittle lock in a
200-unit-high slot. Its rock sill prevents digging below it; the buttress reaches
the water surface so swimming above it cannot skip the heavy crusher. The vent
field beyond is 2,500 units wide (x=18,300–20,800), with a broad, flat floor at
y=-10,000 and open water above. Its labelled crossing is 1,000 units high, with its floor and midpoint
in the Deep band. Three small coral clumps are 200 units tall and 1,170 units
apart. `world_shape.rs` checks this as a wide crossing against the taste bible's
1,500-unit visible-refuge limit. The level ends at x=21,000.

Thirteen sealed `Frame` landmarks reserve the home burrow (600 by 300, below the
cellular opening window, under home), mantis mouth, descending clay tunnel, bone
hall (1,080 by 500), seep shaft,
long chimney, buried chimney return, chimney hall and mouth, odd tunnel, short
Plunge shaft, Plunge crack (300 by 150), and its dark observation stage. Each has
at least two separately labelled `Plug` closures. The chimney cap is brittle but
has a solid backing, so cracking it does not open the unfinished strata. Frames
can contain the generator's substrate; W3.0 installs the strata inside them.
The Plunge's viewing wall is 100 units thick with a twelve-unit slit. Neither a
route nor a dig reaches a frame interior.

The seep's upper outlet and the hanging garden are left to W4.1 (a rock package
then): a first placement sat across the glass-grip climb from the doorstep to the
notch. Frames keep clear of every existing passage (the crack, the pit shaft's
coral, the notch's bare-rock face and the opening window); the tests that walk
those passages with the biggest crab catch an intrusion.
The long chimney has a continuous buried return from the vent bank to home.
No creatures, rewards, keys, tether or rides are added by this package.

### Seep chimneys and the long chimney home (W4.1, W4.2)

Rides are **lanes** (`crates/sim/src/ride.rs`): chains of rectangles with a flow, found from the frames' names by
`crates/sim/src/chimney.rs`, so they follow the frames wherever the world's shape puts them. While a crab's body
centre is inside an open lane, its own physics step runs with its velocity drawn into the flow: it is carried
continuously (never teleported), steers a little across the flow (walk keys; up and down for a sideways flow), cannot
swim against it, spends no stroke or reserve and takes no damage. The flow eases over the last stretch and bends
towards the nearest refuge, so the crab is set down at the exit. In the game, drifting silt specks in the lane's own
rects show the flow (`src/chimney.rs`); no bubbles, no air, no filter.
- **Seep chimney** ("seep shaft"): free from the start, one way up, about 6 s. Its exit must lead only to places the
  crab's kit can already reach on foot (the proofs lane checks it on the real map).
- **The long chimney home** (chimney hall, then the chimney return westwards, then the long chimney up): about 35 s
  from the hall to the shelf. Closed until the crust plug at the chimney's top is cracked by a fully charged heavy
  crusher smash from below (the plug is a brittle rock, shared like any); it cannot be cracked from above. A crab that
  rides it home from the hall has reached **act three** (`Sim::act_three`, saved on its own progress); the Plunge
  reuses the lanes and the ride for its fall.
- While the named frames are sealed bedrock no crab meets a lane; the geometry batch opens their interiors.

### The Plunge (W5.1 to W5.3)

A secret after act three; the beats are in `world-vision-brief.md`, "The Plunge". Rules: `crates/sim/src/plunge.rs`;
drawing: `src/plunge.rs`. Found by the "odd tunnel", "Plunge shaft", "Plunge crack" and "angler stage" frames' names. You
cannot die in it: while carried, in the crack or on the vent nothing reaches the crab. 30 s of holding dig in the odd
tunnel breaks the way; a weak lip pull (the crab can walk out) then the grab; a 30 s fall (strain to full, cracks stay);
the crack, a refuge; after 3 s still, the angler scene once; a small blue-green light (a lantern colour) once; a light
far below blinks twice once; then a 20 s vent ride home to the odd tunnel.

**Not ready to ship:** the old `exploration.rs` slope assertion currently ranges
over every floor segment east of x=8,800. It must be limited to the original reef
ending at x=17,600 before this intentionally steep extension can pass. That file
is outside #487's permitted edit list. Keeping that assertion global instead
requires a long gentle descent that exceeds the cellular bake's fixed
16-million-cell bound. The compact layout fits (15,978,038 cells). The full
9,000-step seed bake, compaction, committed digest update and post-bake canonical
cell tests remain pending; the current committed seed is deliberately unchanged.
The frame dig checks currently exercise raw substrate by changing only the far
boundary to opt out of the stale seed. They must also be run with the canonical
boundary after the final bake.

Pre-bake measurements: all seven lower-canyon tests pass in 19.7 seconds,
including all 112 ability-kit combinations at the narrows and real-cell digging
from both sides. The 1,000-point vent-water grid's worst visible-refuge distance
is 930.5 units (limit 1,500). The thirty-step material probe conserves 81,898 sand
and 58,725 mud cells, takes 2.436 seconds, and projects a 730.8-second full bake.
Formatting and native/wasm clippy pass. The sim suite currently reports 209
passed, 34 ignored, and 227 failures: 225 stale-seed checks, the intentionally
unchanged probe digest, and the legacy slope assertion described above. Workspace
tests stop at four stale-seed fixture failures. These are pre-bake results, not a
claim that the package is ready to merge.
The final complete `world_shape` run passes all 18 active checks (one existing
exhaustive gate matrix remains ignored), but takes 94.1 seconds: its unchanged
home-to-basin narrow-crossing survey exceeds the requested roughly 60-second
file budget. That survey still needs bounded sampling without weakening coverage.

## Lower canyon tether and deep food (W1.4 / #460)

Reaching the vent field (x=18,300–20,800, y=-10,000–-9,000) opens
one shared return current. The current occupies the existing water between
the slump face and the narrows buttress, x=17,781–17,960, from the floor to
y=-2,300. Walk back through the broken narrows, jump into the current and
steer left over the slump crest to Coral Rise's end. It uses the ordinary
water-drag physics at the existing upwelling speed, with no teleport, new
rock, floor edit or altered movement tuning. Until the first visit it is off.
A zero-area marker inside sealed bedrock stores its flag in the existing
net-cut list, so the event, late joins and world switching use the same
transport as cut nets. Remote crabs are never stepped to activate it.

An unnamed deep-food clam at (18,450, -9,970) is visible from the nearest
coral refuge, about 70 units away with clear sight. It uses the ordinary
shared clam bed, carrying slots, pickup preferences and regrowth. The taker's
meal provenance follows drops and cast-off shells. At home it is stored
separately from recipe ingredients, then eaten under the normal meal rules
(including auto-eat and already being well fed). It calls `adapt_deeper(1500)`
once per crab; collecting it, dropping it, or resting elsewhere grants no
adaptation. A friend can obtain the regrown food. The unnamed species and
1,500-unit meal effect are **not verified** biology; the inspiration is TMAO
in `ocean-shaped.md` row 13. No save format is changed; like the existing
strain/adapted-depth fields, this new meal provenance is currently session
state, not part of the legacy `Progress` save payload.

The route survey starts at the slump crest with the crusher lock already
broken (the separate gate test proves that action), descends through the
vent floor and returns to Coral Rise's end. Measured: **0 strokes down,
5 strokes total, minimum reserve 0.462 (3.23 of six reserve strokes spent),
62.36 seconds, zero rest stops**. With only the tether event disabled and
identical terrain and tuning, no six-stroke route gets back over the slump.
This measures the lower-canyon branch, not the previously authored journey
from Coral Rise all the way to the home shell.

Taste check: key visible from refuge **yes**; cheap descent and a real
pre-discovery return debt **yes**; vent field is a round trip via the tether
**yes**. No loop is added. W4.2's long chimney from the vent bank's chimney
hall remains the deferred second way home. The earlier W1.3b pre-bake notes
above are historical: starting commit `f7c6e81` already contains the rebake
and the bounded slope assertion. Its library suite has seven existing
failures (the whole-map memory-save size and six cellular/moray checks):
an untouched checkout and this package both produce 429 passed, 7 failed,
34 ignored. Workspace tests also stop at the same two `cooptest` large-save
fixture failures on both versions. Fixing those save/engine failures is
outside this package. The final focused checks pass: `lower_canyon` 11 tests
in 22.0 seconds; `dive_budget` 6 tests in 10.7 seconds (one survey ignored).
Formatting and native/wasm clippy with warnings denied pass.
Measured: 112 ability-kit combinations at the narrows and digging from both sides; the vent field's worst visible-refuge distance is 930.5 units (limit 1,500); the full seed bake takes about 12 minutes and stays under the bake's 16-million-cell bound.
