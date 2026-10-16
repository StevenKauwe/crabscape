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
  deeper **reserve** (6 strokes' worth, a small bar in the top-left corner) that comes back by itself in about 2
  minutes; when it's empty the legs stay tired, so swimming can't be endless. Floor jumps are free.
- **Sideways is fast.** Crabs scuttle sideways quickly, so the dodge is a short sideways burst. It goes the way
  you hold and needs a direction: dodge on its own does nothing, so it never goes a way you didn't mean.
- **Most of the sea is faster than you.** Fish and shrimp out-speed you; you out-hit and out-last them.
- Possible later: wall cling (crabs grip rock with their dactyls).

## Combat (not built yet)
- **Commit to every hit.** Claw strikes have a clear wind-up, a crushing impact and a recovery. Timing beats speed.
- **Guard with your claws.** Raised claws and a hard shell block blows, the way real crabs defend.
- **Parry:** block at the last moment and the attacker is thrown off balance, opening a window for a crushing
  counter (Expedition 33-style timing).
- **Enemy tells come from real behaviour.** A mantis shrimp cocks its club before it strikes; an anglerfish's lure
  goes still before the lunge; a jellyfish drifts into reach. Learning to read the ocean is how you get good.

## Controls today
| Action | Keyboard | Controller | Phone |
|---|---|---|---|
| Walk | A / D, arrows | left stick, d-pad | `<` `>` |
| Jump (hold = higher) | Space, W, Up, K | A | JUMP |
| Swim (double jump, then more strokes; one per egg eaten) | jump again in water | A again | JUMP again |
| Dodge (on floor, + a direction) | Shift, J | B, X, triggers | DODGE |
| Art style | P | | "Style" |

## Architecture rule
Nothing moves the crab directly: input sends a `MoveRequest` message and fixed-step physics (1/120 s) acts on it, so
co-op can later feed remote players through the same path. Keep it that way.
