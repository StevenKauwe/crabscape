# Common shore crab, a juvenile (*Carcinus maenas*)

Rig: none yet. A stand-in of simple shapes with an ink edge, like the gathering nodes (`src/creatures/shore_crab.rs`): a green carapace with a paler back, eyes on stalks, three legs a side, and claws that rise and spread in the tell. Shared rules: `README.md`.

## Biology that matters
- **Where and what:** the green crab is a small crab of intertidal and shallow rubble and shores, an opportunistic
  scavenger and predator that pinches when threatened. All of this is from general knowledge: **not verified** (no
  source opened for this sheet).
- **Size:** a juvenile with a carapace of about 4 cm (**not verified**); game body 26 x 15 units (1 cm = 6.25).
- **Pinch tell:** raising and spreading the claws as a threat display is common in crabs; for this species
  **not verified**. The tell is a design choice.
- Colours (green-brown carapace) are the species' usual look, **not verified** here.

## Role and register
**A passive roamer on the gather route.** It teaches the fight loop (read the tell, dodge, punish) at almost no
risk, and is a quick, optional fight on the way to work. Cozy: a small neighbour picking at the floor; brutal: it
does pinch; surreal: the sea floor is full of tiny opinions.

## Combat design
- **Passive:** it never starts a fight. A crab that walks or gathers beside it, even standing against it, is never
  touched. A blow on it wakes it: it turns to the crab nearest the blow and fights back.
- **Gives up:** the crab more than 160 away, or 10 s without a blow from the crab. Then it roams again.
- **Pinch:** tell 36 ticks (0.3 s, claws spread wide), active 3, damage 2.25 (a quarter of the hermit's 9), 4
  poise, parryable, blockable. Recover 40 ticks (the punish window). A pause of 20 to 50 ticks sizing the crab up.
- **Weak:** health 26, poise 12 (a smash breaks it), stagger 60. A level 1 crab beats it in 3 smashes.
- **Respawn:** 90 s. **Win XP:** the snapping shrimp's flat Fighting win. **Drop:** none (its body is food; drops
  are certain, not a chance, and a claw per win would feed the claw recipes).

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle (roam) | - | walks 20 u/s to a goal on the other side of home within `LEASH` = 120, then picks at the floor 180 to 540 ticks |
| Notice (turn) | 10 | after being struck |
| Approach | - | 60 u/s until the crab is within 40 of its head |
| Tell | 36 | `Cue::Tell` |
| Attack | 3 | circle at the claw |
| Recover | 40 | open: the punish window |
| Stagger | 60 / 80 parried | |
- Being struck is noted in the `creatures::takes` hook and acted on in its next step. Pauses and goals come from the
  sim's seeded rng, like the other creatures. State, net and hash follow the hermit crab (`Extra::ShoreCrab`).
- **Placement:** none yet; the world lead fills `shore_crab::spots(level)`. `shore_crab::LEASH` and
  `shore_crab::PASSIVE` are public for it. A passive roamer is exempt from the economy tests' node danger distance
  (danger 0): it never strikes first.

## Scenario tests (`crates/sim/tests/shore_crab.rs`)
Roams within its leash and both sides of home; a crab beside it for 60 s, walking through it, and gathering beside
it takes nothing; struck, it fights back and a scripted level 1 crab beats it in under 15 s earning Fighting XP; a
pinch costs a quarter of the hermit's; it gives up at 200 away and after 10 s; respawns after 90 s; deterministic.
