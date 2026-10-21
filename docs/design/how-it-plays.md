# How it plays

Silksong-style movement and combat, but you are a crab: **a heavy-hitting tank.** The feel is momentum, power,
timing and anticipation, not fast and smooth. **Never sluggish, always heavy.** Every movement feel number lives in
`crates/sim/src/tuning.rs` (camera and light in `src/tuning.rs`); `cargo crabsim` measures them headless.

## Movement (the first four are in the game)
- **Weight you can feel.** A crab is denser than water, so it sinks. Jumps are strong push-offs from the floor with a
  short wind-up that the water soon brakes; the way down is a slow, steady sink, not a drop (about 0.8 s back down
  from a 0.5 s rise). Heavy landings bleed speed.
- **Paddle legs give the air moves.** The flattened 5th legs of swimming crabs drive the double jump and swimming,
  one strong stroke at a time, sinking in between (never hovering). Strokes have a rhythm, so mashing doesn't help.
- **Swimming takes stamina; the way up is the cost.** Sinking is free, climbing back up isn't. Every paddle stroke is
  an upgrade: the crab starts with only the floor jump, and each **fish egg** eaten (drifting beads with a glowing
  yolk) grows one more stroke. The first is the double jump, the rest swim higher. The test level has three eggs,
  each reachable only with the strokes the ones before it give (the first unlocks the double jump the trench and
  the high ledge need). Strokes only rest on something solid, so ledges and rocks are rest stops and
  high or hidden routes need planning. Small rings above the crab show them, only while tired. Resting draws on a
  deeper **reserve** (6 strokes' worth, a small bar in the top-left corner) that comes back by itself in about 30
  seconds; when it's empty the legs stay tired, so swimming can't be endless. Sprinting drains it too. Floor jumps
  are free.
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
  way home from the deep when the stamina is spent.
- **Most of the sea is faster than you.** Fish and shrimp out-speed you; you out-hit and out-last them.
- Possible later: wall cling (crabs grip rock with their dactyls).

## Combat (being built: issues #64–#68)
In the sim now: the crab's shell (health) and poise, hits with knockback and hit-stop, and the two claws: the
crusher's **smash** (14-tick wind-up, 6 active, 22 recovery; 12 damage, 25 poise) and the cutter's quick **jab**
(6/4/12; 5 damage, 8 poise). A swing plants the feet (the crab slides to a stop: momentum); dodge cancels its
recovery. Numbers: `crates/sim/src/tuning.rs`. Dying sinks the crab into the sand and brings it back after 1.25 s at
its last safe spot (where it last stood on the floor with no enemy near), until the moult rule.
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
**The combat is preset G** (`Tuning::default`); the feel lab is hidden (`?lab`, or backtick in previews).
**Feel lab (hidden; `?lab`, or backtick in preview builds):** the presets the combat was chosen from. A FEEL button under "Start over" (or the backtick
key, or 1–9) cycles nine presets (A Current, B Heavy tank, C Snappy brawler, D Momentum, E Silksong-ish, F Expedition
timing, and the blends G Momentum + Silksong + Expedition, H Heavy + Silksong + Expedition, I Charge brawler): each a row of swing timings and extras in `crates/sim/src/feel.rs` (lunge, chaining a press late in a
recovery, recoil, a hop on a hit, charging the smash, momentum, the counter's payoff) and of juice in `src/feel.rs`
(anticipation pose, camera shake and kick, flash, shell chips, sound). "Juice off" shows the bare timings. Damage and
poise are the same in all of them: brutal is how a blow lands, not bigger numbers. In all but A a counter **crushes**:
the crusher clamps on, squeezes three cracks into the shell, then lets go with the full blow. Claws never swap sides (the
crusher is the crab's right, screen-left from the front): to strike across, the crab pivots round and shows its back
in a sidestep that carries it 20–40 units into the blow (10–16 ticks per preset, the wind-up overlapping its end;
presses during it are kept). From behind, what sits on the front of the body (claws, lantern, eye stalks) goes behind
the shell; the legs stay tucked under its edges, their order among themselves reversed (the paddles come forward). It stays turned until it needs the other side or has been out of the
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
| Zoom out into the crab's memory (`crab-memory.md`) | - / =, mouse wheel | | pinch |
| Art style | P | | "Style" |
| Feel lab: next preset / juice off-on (previews) | backtick (or 1–9) / backslash | | "FEEL" / "Juice" |

## Architecture rule
Nothing moves the crab directly: input sends a `MoveRequest` message and fixed-step physics (1/120 s) acts on it, so
co-op can later feed remote players through the same path. Keep it that way.
