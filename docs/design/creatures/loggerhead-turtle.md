# Loggerhead sea turtle (*Caretta caretta*)

Register: **open water hunter**. The one lesson: **look up.** The threat comes down from above; cover beats guard.
Shared rules: `README.md`. Code: `crates/sim/src/creatures/turtle.rs`, tests `crates/sim/tests/turtle.rs`.

## Biology that matters
- **Size:** average carapace length 85-100 cm, adults mature past 90 cm; mass 77-545 kg, average 135 kg [ADW].
  NOAA gives adults 2.5-3.5 ft (about 76-107 cm) and 200-350 lb [NOAA]. Game adult: **90 cm straight carapace length**
  (562 units), the head out past it, hurtbox 656 x 187 over shell and head (1 cm = 6.25 units). Size audit: the
  shell was drawn 77 cm, the 90 cm having been shell plus head.
- **Diet:** carnivore; juveniles and adults in coastal water eat mostly bottom-dwelling invertebrates: mollusks,
  crustaceans, horseshoe crabs; whelk and conch are named hard-shelled prey, and "their powerful jaws are designed to
  crush their prey" [NOAA]. ADW lists horseshoe crabs, bivalves, barnacles, whelks and conchs, and says food is found
  by sight or smell [ADW]. A 12 cm swimming crab as prey: the pages name crustaceans, not this crab; a design reading.
- **Air:** "must come to the surface to breathe" [NOAA]; a typical dive is 4 to 5 minutes, up to 20, to a recorded
  61 m [ADW]. In the game the **crab never breathes air**; the turtle going up is just it leaving. Never drawn with
  bubbles. Its tuned "dive clock" (75 s of hunting) is not a measured dive length.
- **Defences:** adults are protected by shell, size and rough skin; sharks occasionally take them [ADW]. In the game
  it is driven off, never killed.
- **Swim speed, dive angle, eyesight range, how it takes a crab:** not verified. The head-down glide, swept-back
  front flippers and the 3.5 s tell are design readings of a heavy animal turning to a bottom prey.

## Role and register
A big slow animal that wants a meal, not a fight. It patrols high over the shallows and the cliff by the basin,
notices a crab on the open floor, and comes down on it once or twice. Hunters here are not about guard: the beak
**cannot be parried** and a block cracks the guard. The answers are all *positional*: be somewhere else, or under
something.

## Combat design
- **Attack:** one crushing bite at a point locked in the last half second of the dive. 30 damage, 60 poise,
  `armour_break` 600 ticks (like the mantis club). Not parryable.
- **Tell (3.5 s):** it pitches head-down and glides in, front flippers swept back; the pitch grows as it falls.
  Sound: the shared tell swell (`Cue::Tell`). It falls at least 600 units, so the first warning is a shape coming
  down the screen. (The brief's "shape grows as it descends" is not drawn: size never changes; the pitch and the
  closing distance carry it.)
- **The crab's answers:**
  - *Walk or sprint off the locked point* in the last 0.5 s (a walk is 135 units in that time; the bite reaches
    111 from the lock, centre to centre).
  - *Dodge* through the 10-tick bite (dodge invulnerable ticks 2-13).
  - *Cover:* a solid between the mouth and the crab (a ledge, an arch, a slab) stops the bite; a crab in a Refuge,
    Mouth or Tunnel landmark, or gripping kelp or coral, is not noticed, and if it gets there before the lock the
    turtle breaks off. After the lock it commits and misses.
  - *Block:* no damage but the guard cracks for 5 s and 60 poise goes; the second pass then bites through a cracked
    guard. This is the "block is wrong here" lesson.
- **Punish:** a miss leaves it hanging low, level, for 180 ticks (1.5 s) with its head out: hits at the head count
  x1.5. A bite that reached the crab recovers in 90.
- **Leaves** after 2 passes, when hurt below 60% of its health, or when its dive clock runs out: swims up and away,
  then is gone for 4 minutes. Poise (160) breaks under about 7 smashes and staggers it 150 ticks.
- **How it differs:** the only fight where parry is not the answer and where cover, not a guard, wins; the only
  hunter that dives from above and never wants a long fight.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle (patrol) | - | Glides +/- 250 units about home at 90 u/s at its authored depth. Checks every 8 ticks for a crab it sees |
| Notice | 90 | Turns toward the crab (0.75 s) |
| Approach (stalk) | - | 300 u/s to a hover point 800 above the crab; tell starts within 200 of it |
| Tell (dive) | 420 | Closes on the point where its mouth meets the crab's middle; tracks the crab until tick 360, then **locks**; arrives exactly at tick 420 (speed <= 520 u/s, else it falls short and misses) |
| Attack (bite) | 10 | Circle r 75 at the mouth (270 ahead, 30 below its feet point). Cover is judged on tick 1 |
| Recover | 180 miss / 90 hit | Drifts at 60 u/s; head x1.5 |
| Stagger | 150 | Poise broken |
| Flee | - | Up and away at 260 u/s, 1,500 units (or the surface); then Gone for 28,800 ticks |

- **Numbers:** health 300, poise 160 (refill 20/s after 90 ticks), bite 30 / 60 poise, knockback 260. Hurtbox 560 x
  160. No single bite kills a full 100-shell crab; two undefended passes do 60.
- **Real vs tuned:** size is real; everything else is tuned. It sees a crab 1,100 units away (a lit lantern: x1.35;
  a dimmed one: the usual dimming rule), needs a clear line of sight, and lets go of a crab more than 900 sideways
  from home. It hunts only inside that leash, so the world-shape tunnel envelope (leash plus tell travel) stays
  clear of every tunnel; it also never glides into a tunnel interior (the shared sanctuary guard).
- **Co-op:** blows land only on `!remote` crabs; `feels` is its only reaction (landed = a blow reached the crab);
  recovery length is read from that, so a late-arriving outcome still converges.
- **Drops:** none (`DROPS` empty).

## Where it lives, and the encounter
Placed by landmark name: 250 units west of the "basin" open water, 700 above the cliff top (about (2551, 1560)),
over the shallows' east end and the cliff. A walker crossing the plateau toward the cliff is hunted; the basin's
return ledges (Refuge landmarks, with real ledge overhangs) are the cover; the home refuge and kelp are cover too.
Nearest refuge to its home is more than 400 units away. It teaches: **look up, move early, get under something.**

## Rig notes
Parts: `body` (shell, scutes), `head` (+`eye`), `flipper-front-near/far` (long), `flipper-hind-near/far`, `tail`.
Poses (all read from the sim, no overlays): idle/stalk slow flipper stroke; tell pitches the whole animal up to
about 45 degrees about the mouth while the front flippers sweep back; bite steepest, head dipped; miss hangs level,
head lifted; stagger wobble; flee nose up. Hidden when `Gone`.

## Scenario tests (`crates/sim/tests/turtle.rs`)
Placement by name; tell 420 ticks with `Cue::Tell` and no damage before the bite; the bite lands on the locked point
for exactly 30; two undefended passes leave the crab alive and it leaves; a slab overhead takes no damage; a refuge
crab is not noticed and a crab reaching one mid-dive is not bitten; block gives no damage, cracks the guard, cannot
be parried; dodge and walking off the lock take nothing; a miss recovers 180 ticks and the head takes x1.5; poise
breaks and low health sends it up; the dive clock sends it up; a crab beyond the leash is let go; never inside a
Tunnel or Mouth; determinism; a two-device co-op run.

## Sources
- [NOAA] NOAA Fisheries, Loggerhead turtle. <https://www.fisheries.noaa.gov/species/loggerhead-turtle>
- [ADW] Animal Diversity Web, *Caretta caretta*. <https://animaldiversity.org/accounts/Caretta_caretta/>
