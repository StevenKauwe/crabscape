# Bobbit worm (*Eunice aphroditois*)

Rig: `crates/crabrig/src/bestiary/bobbit.rs` (ink only, no concept sheet yet). Shared rules: `README.md`. Issue #581.

## Biology that matters
- **Size:** under 10 cm to nearly 3 m; the largest recorded was 299 cm but only about 25.5 mm wide [WP-EA, citing
  Kubota et al. 2009: the paper itself not opened]. Game: 1 m (625 units) of body, almost all of it buried; the head
  2.5 cm wide (16 units; head width not verified, the body width is the sourced figure), 18 units high with the jaws.
- **Where:** warm Atlantic and Indo-Pacific seas, coral reef, sand and mud, among rocks and sponges; 1-200 m
  [SLB], to 95 m in the other record [WP-EA]. Benthic. Game: the sand flat east of the octopus's den.
- **Ambush:** it buries its body in soft sediment, only the head end out, and waits until an antenna senses prey; it
  has five antennae on the head [WP-EA citing Encyclopedia of Life; popular accounts agree, e.g. Live Science].
  It has little or no vision; the antennae carry chemical and light receptors (popular source: not verified).
- **Strike:** it grabs the prey with its jaws and drags it into the burrow [WP-EA citing Haag-Wackernagel & Lachat
  2016]. The jaws are sharp mandibles that retract into the body and can snap some prey in half (Encyclopedia of
  Life via [WP-EA]). Slow-motion footage shows sand spraying and prey vanishing into the burrow (popular source).
  No measured strike speed was found: not verified.
- **Diet:** mainly a carnivore on many fish species [WP-EA]. Crabs as prey: not verified. Fish defend by mobbing
  the burrow with water jets [WP-EA citing Haag-Wackernagel & Lachat 2016].
- **Nocturnal:** not verified (the source pages open did not say); the game gives it no day/night rule.

## Combat design
**Role: micro-zone ambusher, a minefield you read.** It never chases. Two worms 250 apart sit in one patch of
sand; the lesson is to read where the antennae stand, not to walk into their touch, to bait the snap from outside
its reach, then punish the neck.
- **Senses (tuned):** *touch* 70 units around the antennae (a crab's body edge, even standing still); *vibration*
  150 units for a crab sprinting (over 300 u/s), dodging or landing from a hop. Walking (270 u/s) outside the
  touch radius is not felt. A crab above it (a hop over, or high swimming) is out of the antennae's reach. A crab
  holding kelp or coral is not felt. It does not see: the lantern and ink make no difference.
- **Tell (0.5 s, short because the patch is readable in advance):** the antennae flick upright and spread, the
  sand mound bulges, then the head lifts and the jaws show. A `Cue::Tell` swell plays for the crab.
- **Strike:** the head and jaws shoot out along the sand toward the crab, 4 active ticks, ending 100 units from the
  burrow (a crab's centre is hit up to 136). 16 damage, 35 poise; parryable and blockable (block lets 20% through,
  and a blocked bite never grips). A bite that gets through **grips**: it drags the crab slowly toward the burrow
  (never closer than 56 centre to burrow), 4 damage every 40 ticks, up to 168 ticks (1.4 s, with hit-stop under
  1.5 s); mash dodge or drop a leg (down + dodge) to get out.
- **Answers:** *parry* (staggers it, neck out); *block*; *dodge* through the 4 ticks (press about 3 ticks before
  the tell ends); *hop over* it; *bait* it (hop or sprint just outside 136 units, inside 186): it lunges at nothing.
- **Weaknesses:** buried it cannot be hit at all. After a miss or a parry its neck stays out for 1.2 s (144 ticks
  recover, or stagger 72 + recover 48) and the neck takes x1.5 (a smash does 18). The head and neck are the whole
  hurtbox. Hurt below 40% it withdraws and stays buried for 20 s, then comes back whole.
- **What it teaches / how it differs:** unlike the moray (smell, a crevice, a long tell) and the mantis (a hunter
  that darts), it is a fixed position you can see around: the fight is picking where to stand.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle (buried) | - | `Enemy::hidden`: only antennae show, not hittable. Senses each tick (after a 120-tick re-arm when it has just drawn back in) |
| Tell | 60 | Aim locked at the crab's side when felt; last 20 ticks "jaws" |
| Attack | 4 | Hit circle r 14 travels 78 units from the head (`lunge`); crab outside 136 is never hit |
| Grab | <= 168 | Bite 4 every 40 ticks; drags 45 u/s; ends when the crab is free, then Recover 72 |
| Recover | 144 | Neck out; neck x1.5. Then draws back in over 24 ticks |
| Stagger | 72 parried / 40 poise break | Then Recover 48 (or withdraw if weak) |
| Flee (withdraw) | 24 + 2,400 | Below 40% health: draws back, buried 20 s (hit-proof), then full health |

- **Boxes:** hurtbox 16 x 18 growing to the head along `reach`; strike circle r 14, 10 up. Placement: never moves.
- **Numbers:** health 40, poise 30, damage 16, poise 35, grab 4/40 ticks, flee below 40%, respawn after 6 min.
  Real: size, burial, five antennae, grab and drag. Tuned: radii, all timings, damage.
- **Drops** (`DROPS`): "bobbit jaw", plus food.
- **Placement** (`spots`): two worms 250 apart on the flat just east of the "octopus den" landmark (the far rim's
  sand). 400+ from home, ledges, crevices and landings; 150+ from coral stands, tunnels, mouths and chambers; 150+
  from clams and grit; never in a Tunnel or Mouth. The far basin was tried and rejected (it holds the mantis mouth
  and seep shaft); the eastern reef's other flats are all clam beds or other habitats. This is next to the
  octopus's den, 300 from its home: if that feels stacked, move the pair.
- **The patch's safe lines:** a body standing exactly between the two (125 from each) is not felt walking; getting
  there takes a hop over the first worm (the landing is felt from 150: land outside it) or baiting and driving the
  first one off. Withdrawn worms are not felt for 20 s.
- **Scenario tests:** `crates/sim/tests/bobbit_worm.rs`.

## Rig notes
Parts (`crabrig/src/bestiary/bobbit.rs`): four segments (teal and rose iridescence, bands, a sheen), the head
(two eyes, a palp) with two mandible parts hinged on its front (the jaw blades are children of the head), five
antennae as root parts, a sand mound. Poses (`src/creatures/bobbit.rs`, from the sim only): buried (antennae stand
on the sand, small), tell (antennae grow and fan, mound bulges, head rises, jaws open), strike (head out at
`reach`, jaws open then shut), recover (head out, jaws working), grab (jaws shut on the crab), stagger (wobble),
drawing back (head sinks), withdrawn (mound only), defeat (body lying on the sand).

## Sources
- [WP-EA] Wikipedia, "Eunice aphroditois" (start point; it cites Kubota et al. 2009, Haag-Wackernagel & Lachat 2016,
  Encyclopedia of Life, SeaLifeBase). <https://en.wikipedia.org/wiki/Eunice_aphroditois>
- [SLB] SeaLifeBase, *Eunice aphroditois*: 1-200 m, benthic, Indo-Pacific and Atlantic; no size or diet data.
  <https://www.sealifebase.se/summary/Eunice-aphroditois.html>
- Live Science, "Sand striker: the glittering 'bobbit' worm..." (popular, via search summary only).
