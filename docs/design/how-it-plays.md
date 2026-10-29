# How it plays

Silksong-style movement and combat, but you are a crab: **a heavy-hitting tank.** The feel is momentum, power,
timing and anticipation, not fast and smooth. **Never sluggish, always heavy.** Every movement feel number lives in
`crates/sim/src/tuning.rs` (camera and light in `src/tuning.rs`); `cargo crabsim` measures them headless.

## Movement (the first four are in the game)
- **Weight you can feel.** A crab is denser than water, so it sinks. Jumps are strong push-offs from the floor with a
  short wind-up that the water soon brakes; the way down is a slow, steady sink, not a drop (about 0.8 s back down
  from a 0.5 s rise). Heavy landings bleed speed.
- **Paddle legs give the air moves.** The flattened 5th legs of swimming crabs drive the double jump and swimming,
  one strong stroke at a time, sinking in between (never hovering). Strokes have a rhythm (0.30 s, quicker with the
  Swimming skill), so mashing doesn't help: a press during the rhythm is never lost, it waits and strokes the moment
  the legs are ready (one at most), so mashing strokes exactly on the rhythm and climbs no higher than presses on
  the beat.
- **Swimming takes stamina; the way up is the cost.** Sinking is free, climbing back up isn't. Every paddle stroke is
  an upgrade: the crab starts with only the floor jump, and each **fish egg** eaten (drifting beads with a glowing
  yolk) grows one more stroke. The first is the double jump, the rest swim higher. The test level has three eggs,
  each reachable only with the strokes the ones before it give (the first unlocks the double jump the trench and
  the high ledge need). Strokes only rest on something solid, so ledges and rocks are rest stops and
  high or hidden routes need planning. Touching down rests one stroke at once (so a hop straight off again has its
  double jump), the rest over 0.6 s of standing. Small rings above the crab show them, only while tired. Resting draws on a
  deeper **reserve** (6 strokes' worth, a small bar in the top-left corner) that comes back by itself in about 30
  seconds; when it's empty the legs stay tired, so swimming can't be endless. Sprinting drains it too. Floor jumps
  are free. **Well fed** (a clam eaten at home, `world.md`), the reserve holds 3 strokes more (pale gold segments)
  and the shell 25% more (a longer shell bar), and "Well fed 3:58" under the shell bar counts down the time left.
- **Sideways is fast.** Crabs scuttle sideways quickly, so the dodge is a short sideways burst. It goes the way
  you hold and needs a direction: dodge on its own does nothing, so it never goes a way you didn't mean. Keep
  holding dodge to **sprint** (a fast scuttle, between walking and the dodge) on the floor; it drains the reserve,
  and when the reserve is empty the crab is winded and walks until dodge is let go.
- **Dive and flip in the water.** Pushing a way while sinking tilts the crab into a dive: streamlined, it sinks a
  little faster and moves across a little quicker. Dodge in the water is a flip, a quick somersault (a flourish for
  now; it doesn't carry the crab further, so the trench and the cliff stay fair).
- **Climb kelp and coral.** In the water, press jump while touching a kelp stalk or a coral to grip it instead of
  stroking. With the walking legs (no stamina): hold jump to climb up, hold down (or dodge) to climb down, let go of
  both to hang on. Sideways shuffles along the stalk (fidgeting is safe); pushing on past the edge steps off; jump with a direction kicks off it. The crab turns sideways onto the stalk, on the side it came from (crabs climb sideways), and swings round to the other side if something is in the way; its body box turns with it. Climbing past the top pulls the crab up over it. Slow but free: the
  way home from the deep when the stamina is spent. **Glass grips** (from a glass sponge, `world.md`) let it grip
  bare rock the same way: the pale, pitted bands on a few walls, which it otherwise slides off.
- **Ride the tide.** In the canyon below the crack the water runs up its middle, then down, a minute a round
  (`world.md`): it carries the crab with it (sinking and swimming are against the water), so the flood lifts it
  with no strokes and the ebb drops it fast, too strong to swim up against. Marine snow streams the way it runs, and in the canyon a line under the
  depth counter says which way ("The water: running up").
  The flood is the only way up off the canyon's floor: steer out of it onto a ledge before it turns.
- **Most of the sea is faster than you.** Fish and shrimp out-speed you; you out-hit and out-last them.
- Possible later: wall cling (crabs grip rock with their dactyls).

## Feats and moulting
A skill at level 5 makes a **feat** ready: a new move, grown only at a moult (`world.md`, Moulting and feats).
Well fed and tucked into the home shell, a "Moult: grow ..." button moults the crab (one feat per moult). It is soft
for 90 s of play (a third of that tucked in): half the shell, claws at 60%, no eating. Hardened, the feat is on and
the shell holds 10 more. The feats (their biology: not verified):
- **Gripping dactyls** (Climbing): the leg tips grip kelp and coral hard enough to brace on, so a kick off a stalk
  becomes a long leap. (Bare rock is the glass grips' business, a fitted gear, not a feat.) Being built.
- **Broad paddles** (Swimming): the flip in the water becomes a real dash that carries the crab. Being built.
- **Heavy crusher** (Fighting): a charged smash cracks brittle rock and breaks poise harder. Being built.

## Combat (being built: issues #64–#68)
In the sim now: the crab's shell (health) and poise, hits with knockback and hit-stop, and the two claws: the
crusher's **smash** (14-tick wind-up, 6 active, 22 recovery; 12 damage, 25 poise) and the cutter's quick **jab**
(6/4/12; 5 damage, 8 poise). A swing plants the feet (the crab slides to a stop: momentum); dodge cancels its
recovery. The claws aim at what they strike: a blow comes down (up to 16 units) onto a creature that is short or a
step below, or up (up to 28) at one a little above, and anything overlapping the crab's shell is struck too; behind
the shell, far above or out of reach it misses. Numbers: `crates/sim/src/tuning.rs`. Dying sinks the crab into the sand and it wakes 1.25 s later tucked in
its home shell, or in the last crevice it rested in (rest stops, `world.md`), with full health and everything it had grown or fitted (lantern, scissor claws, climbing). What it
was carrying (the big find on its back and the clams and grit on its shell) stays in its **cast-off shell** where it
died, drawn pale on the floor with all of it on it: walking back over it picks the load up again, whatever there's
room for ("Got it back!").
A beaten creature's body lies where it fell (belly up) until it comes back: walking onto it, the crab eats it, mending
half its shell and filling its reserve, once per body. Home and the rest-stop crevices are the only other places the
shell mends, so a fight on the way is worth it. Tucked right in (at home or in a crevice) the crab is out of reach: no
blow lands, and a creature hunting it gives up and goes back to its den; backing in or coming out, it can still be hit.
The mantis stalks: walking past or away doesn't escape it, only a sprint or a fight does.
**Block** (hold): damage ×0.2, poise in full, walk at 40%. **Parry**: block pressed ≤ 10 ticks before a blow lands
(a fresh press; a tap counts, a held block never does, and presses closer than 0.2 s apart open no new window):
no damage, the attacker staggers, and a smash *started* in the next 45 ticks is a ×2 counter. **Dodge** (and flip):
invulnerable on its ticks 2–13. Enemies (`crates/sim/src/enemy.rs`) run the bestiary's state machine with a
`tell` whose last stretch is `cocked`, and take the crab's blows by the same rules.
**The test level's creatures** (placeholder shapes until their puppets), easy to hard along the route: two
snapping shrimp in the shallows rubble (they feel your steps within 120; creep past at under half speed; their snap
runs a shock along the floor: jump or dodge it, it can't be parried), the mantis shrimp on the plateau, a snowflake
moray in a crevice of the trench (it smells you after a second within 260, coils into an S and lunges; an unblocked
bite holds on: tap dodge to struggle free), and past the trench a shrimp on the far ledge and a moray on the flats.
**The common octopus** (a boss, `creatures/common-octopus.md`; rules built, not placed yet): a rock that looks back.
Pass within 140 of it and it flushes dark for a quarter second, then pounces and holds on (dodge, parry or block the
pounce). Found out, it fights from its den: arm tips slide in for half a second to touch the crab, two at most. Parry
the touch and the tip recoils; block it and it's pushed back (guarding forever breaks your poise); let it touch and it
grabs (tap dodge to struggle free). Strike an arm and it pulls back; each arm has its own health, and one cut to 0
stays out of the fight. Its eye counts double; scissor claws cut arms twice as deep. Close to it with room overhead,
it rises for most of a second, spreading its web, then drops it like a net: dodge as it comes down (claws can't stop
it); missed, it lies with its arms spread, open to a punish (arm hits x1.5). It can't do that in a low tunnel.
Held by anything of its, at 1 s it bites with venom (2 a second for 5 s, stacking, and half walking speed), at 1.7 s
it bites hard (30). **Autotomy:** while held by anything, down + dodge, or holding dodge half a second, drops a
walking leg and frees the crab at once (8% slower per leg, back after the next wake from death).
Hurt to two-thirds, and again to one-third, it inks and jets away 600 or more to fight on from there; inside the ink
the crab is *blinded*: things that hunt by sight lose it (the octopus's arms still feel for it close by). Beaten, it
jets off for good, and its den is open: a midden of old shells, 4 pieces of shell grit for the home.
**The combat is preset G** (`Tuning::default`); the feel lab is hidden (`?lab`, or backtick in previews).
**Feel lab (hidden; `?lab`, or backtick in preview builds):** the presets the combat was chosen from. A FEEL button under "Settings" (or the backtick
key, or 1–9) cycles nine presets (A Current, B Heavy tank, C Snappy brawler, D Momentum, E Silksong-ish, F Expedition
timing, and the blends G Momentum + Silksong + Expedition, H Heavy + Silksong + Expedition, I Charge brawler): each a row of swing timings and extras in `crates/sim/src/feel.rs` (lunge, chaining a press late in a
recovery, recoil, a hop on a hit, charging the smash, momentum, the counter's payoff) and of juice in `src/feel.rs`
(anticipation pose, camera shake and kick, flash, shell chips, sound). "Juice off" shows the bare timings. Damage and
poise are the same in all of them: brutal is how a blow lands, not bigger numbers. In all but A a counter **crushes**:
the crusher clamps on, squeezes three cracks into the shell, then lets go with the full blow. Claws never swap sides (the
crusher is the crab's right, screen-left from the front): to strike across, the crab pivots round and shows its back
in a sidestep that carries it 20–40 units into the blow (10–16 ticks per preset, the wind-up overlapping its end;
presses during it are kept). The turn coils (a crouch, leaning away from the blow), snaps round (stretched,
lifted, a smear of shell behind it) and lands wide and squashed; the crusher stays tucked through the spin and rears
up once round. From behind it shows the back of its shell (no face; the eye stalks peek over it); what sits on the
front of the body (claws, lantern, eye stalks) goes behind the shell; the legs stay tucked under its edges, their
order among themselves reversed, and the paddles come in front. It stays turned until it needs the other side or has been out of the
fight for 2 s. A streak runs along each blow; the mantis shows its strike line while it tells. The game plays **G**; the numbers above are A (`Tuning::default`, what the tests use).
- **Brutal means bodies break** (a direction, not a spec yet). Hits have located, visible consequences, the same
  rules for the crab and every creature: shells crack, limbs are torn off or dropped, and a damaged part changes what
  its owner can do (a crusher torn off can't smash; a mantis with a cracked club strikes weaker). Damage carries until
  the next moult (#62). Not big numbers or harsh difficulty: fights should end in fewer one-combo deaths and more
  limping. Biology: crabs drop a leg or claw and regrow it over the next moults (regrowth: [KM] in
  `art/crab/anatomy-notes.md`; that they break at a set breakage plane: TO VERIFY); a mantis strikes a shell again and
  again until it reaches the soft parts (`creatures/mantis-shrimp.md`; that strikes shatter crab shells: TO VERIFY);
  crusher claws are built to break shells (TO VERIFY; heterochely itself: [YDF] in anatomy-notes).
- **Commit to every hit.** Claw strikes have a clear wind-up, a crushing impact and a recovery. Timing beats speed.
- **Guard with your claws.** Raised claws and a hard shell block blows, the way real crabs defend.
- **Parry:** block at the last moment and the attacker is thrown off balance, opening a window for a crushing
  counter (Expedition 33-style timing).
- **Claw upgrades become fighting styles.** Found things fitted to the claws: **cut** (scissor claws, a shed tooth's
  edges on the crusher: in the game, cutting ghost nets), **hammer** (the crusher's smash, built up) and **rip**
  (gripping and tearing). Each its own branch to grow.
- **Enemy tells come from real behaviour.** A mantis shrimp cocks its club before it strikes; a snapping
  shrimp holds its claw open; an octopus rises and spreads its web. Learning to read the ocean is how you get good.
  The first six enemies and the shared combat model (health, poise, block, parry, statuses): `creatures/README.md`.

## Controls today
| Action | Keyboard | Controller | Phone |
|---|---|---|---|
| Walk | A / D, arrows | left stick, d-pad | `<` `>` |
| Jump (hold = higher) | Space, W, Up, K | A | JUMP |
| Swim (double jump, then more strokes; one per egg eaten) | jump again in water | A again | JUMP again |
| Dodge (on floor, + a direction; hold to sprint; in the water: a flip) | Shift, J | B, right trigger | DODGE |
| Smash (crusher) | E, L | Y | SMASH |
| Jab (cutter) | Q, O | X | JAB |
| Guard (hold: block; tap just before a blow: parry) | F, I | LB, LT | GUARD |
| Climb (grip kelp or coral in the water) | jump to grip, hold to go up; S / down to go down; sideways to step off; jump + direction to kick off | A, hold; stick down | JUMP, hold; DODGE to go down |
| Go into the home shell or a rest-stop crevice to rest (home also uses what you carried) | walk into its doorway; walk away, jump or dodge to come out | stick, d-pad | `<` |
| Moult (tucked in at home, well fed, a feat ready) | click "Moult: grow ..." | | tap it |
| Zoom out into the crab's memory (`crab-memory.md`) | - / =, mouse wheel | | pinch |
| Art style (Ink or Oil) | P | | Settings, "Art style": Ink / Oil |
| Feel lab: next preset / juice off-on (previews) | backtick (or 1–9) / backslash | | "FEEL" / "Juice" |
| Enter a code (see Codes) | Enter, type it, Enter (Esc puts it away) | | Settings, "Enter a code" |
| Start over (wipes this device's saved game) | | | Settings, "Start over", then "Sure? Start over" within 4 s |

## Codes
A playful cheat, the owner's idea. Typed codes change only your own crab, go through the sim like the controls (a
replay carries them as `code=hulk`) and last until the page is reloaded (they are not saved). Friends see them.
- `hulk` (again to turn it off): the crab bulks up, drawn 1.5× bigger from its feet with a green shell. Its smash does
  3× the damage and 2.5× the knockback, it feels a fifth of a blow's poise damage, and it walks at 0.8× speed. Its
  claws reach 1.5× further (and higher) and its body is 1.5× bigger in a fight; its box against the level stays the
  same so it still fits through every gap.
- `feast`: shell mended, reserve and paddle strokes full.
- `glow` (again to turn it off): the lantern burns as bright as four cultures (sight too); nothing earned is changed.
- Anything else: "Nothing happens."

## Architecture rule
Nothing moves the crab directly: input sends a `MoveRequest` message and fixed-step physics (1/120 s) acts on it, so
co-op can later feed remote players through the same path. Keep it that way.
