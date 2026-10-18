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
recovery. Numbers: `crates/sim/src/tuning.rs`. Dying returns the crab to the spawn after 3 s, until the moult rule.
**Block** (hold): damage ×0.2, poise in full, walk at 40%. **Parry**: block pressed ≤ 10 ticks before a blow lands
(a fresh press; a tap counts, a held block never does, and presses closer than 0.2 s apart open no new window):
no damage, the attacker staggers, and a smash *started* in the next 45 ticks is a ×2 counter. **Dodge** (and flip):
invulnerable on its ticks 2–13. Enemies (`crates/sim/src/enemy.rs`) run the bestiary's state machine with a
`tell` whose last stretch is `cocked`, and take the crab's blows by the same rules.
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
| Dodge (on floor, + a direction; hold to sprint; in the water: a flip) | Shift, J | B, X, triggers | DODGE |
| Climb (grip kelp or coral in the water) | jump to grip, hold to go up; S / down to go down; sideways to step off; jump + direction to kick off | A, hold; stick down | JUMP, hold; DODGE to go down |
| Art style | P | | "Style" |

## Architecture rule
Nothing moves the crab directly: input sends a `MoveRequest` message and fixed-step physics (1/120 s) acts on it, so
co-op can later feed remote players through the same path. Keep it that way.
