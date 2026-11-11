# Amphipod swarm (scavenging lysianassoid amphipods; issue #584)

Shared rules: `README.md`. Module `amphipods`, `Kind::AmphipodSwarm`, art name `amphipod-swarm`. No concept sheet yet:
the rig is an ink-only placeholder.

## Biology that matters
- **What they are:** scavenging amphipods of the lysianassoid kind (*Eurythenes*, *Orchomenella*, *Hirondellea*). Game
  individual: **2 cm (12.5 units)**. That is a game choice for the small kinds (*Orchomenella*, *Hirondellea* sizes
  not verified). We deliberately do not use the big *Eurythenes* size for a crowd.
- ***Eurythenes gryllus* size:** Wikipedia says up to ~15 cm, citing the Marine Species Identification Portal;
  SeaLifeBase gives a maximum of 5.6 cm (males/unsexed) and 7.5 cm (females). The sources disagree, so a single
  "big" size is not verified. Opened: [W-EG], [SLB-EG].
- **Where:** benthic, 550-7,800 m [W-EG, citing Havermans et al. 2013]; SeaLifeBase 0-5,800 m [SLB-EG]. Fits the
  lower canyon floor.
- **Diet:** predator and scavenger [SLB-EG, W-EG]. Caught with baited traps [SLB-EG]. That they strip soft tissue
  from a carcass in huge numbers within minutes to hours: widely reported by baited-camera studies, but **not
  verified here** (no baited-camera paper opened; Wikipedia's *Lysianassoidea* and *Hirondellea gigas* pages did not
  load).
- **How they find food:** smell. *E. gryllus* has "highly developed long-range chemoreceptive tracking" that lets many
  gather at sunken carcasses quickly [W-EG, citing Havermans et al. 2013]. Fast swimming also from [W-EG].
- **Eating a weakened or dead crab:** plausible for scavengers; **not verified**. "A wound smells" is a design rule.
- **Swarm of 120:** tuned. Real trap counts run far higher; 120 is what a phone can draw and read.

## Combat design
**Role: micro-zone crowd.** A swarm you cannot fight, only outwalk. Lesson: **wounds smell**. A hurt crab draws a
cloud; keep moving, leave food, or get out of its reach.
- **One enemy is the whole cloud** (200 x 80 units, 120 individuals in its own `State`). It is dormant, scattered in
  the sediment and **cannot be hit**, until something draws it.
- **What draws it (smell):** a crab below **70% shell** within 900 units (up to 1,200 as it gets more hurt: the
  swarm will not travel past its 1,200 leash, so smell farther than that is moot); **or food lying about** within
  900 units (a clam in a cast-off shell or a deliberate drop). Food comes first: it goes to the food, feeds 20 s, and
  ignores the crab meanwhile.
- **Tell (rising), 360 ticks (3 s):** the swarm rises out of the floor, a shimmering cloud, a few at first. A
  `State::Tell` (the swell cue plays when a crab is the target). Nothing touches the crab during the rise.
- **Approach:** drifts at 90 u/s (the crab walks at 270). Contact is the first time the cloud box touches the crab,
  at least 3 s after the first rise.
- **Nibbles:** while the cloud box touches the crab, one tiny Hit every 60 ticks (2 shell per second) of 1 damage,
  0 poise, no knockback. Not parryable (no stagger to give); blockable at x0.2; a dodge's invulnerable ticks avoid
  one. A nibble does not stun or freeze the crab (the sim restores the knock and hit-stop its hit gave).
- **The swarm never kills:** a nibble is capped so shell never goes below **25**. At 25 it has had its share of that
  crab (it is full, drifts off, and no longer smells it). It is also full after 30 landed nibbles (a blocked nibble
  counts a fifth).
- **The crab's answers:** *walk away* (it gives up past 1,200 units of its patch, and a crab outpaces it three to
  one); *leave food* (drop a clam: the swarm feeds on it 20 s instead); *a refuge*: it never touches a crab in a
  Refuge/Mouth/Tunnel landmark, or one climbing kelp/coral (the coral stand a few hundred units east of the patch
  is one); *block* (a fifth); *dodge through*; or *fight*: each smash (12 damage) kills 6 individuals (health 240,
  2 per individual).
- **Weaknesses:** slow; blind to nothing (smell, so an ink cloud does not hide the crab); no poise; cannot enter
  tunnels or mouths; scattered below **30% count**: it flees into the sediment and leaves a catch.
- **Feel:** cozy: it is a floor of tiny busy lives; brutal: a hurt crab is being quietly eaten; surreal: a cloud
  that has a will.

## Implementation spec
Skeleton states, as mapped (`phase()` names in brackets):

| State | Ticks | Notes |
|---|---|---|
| Idle at home [dormant] | - | Hidden and unhittable (`burrows`). Re-reads the smell every 15 ticks. Regrows 1 health per 30 ticks when unmolested for 2 s |
| Tell [rising] | 360 | Target set when a crab draws it; abandoned (settles) if the smell is gone |
| Approach [drifting] | - | 90 u/s toward the crab or the food; gives up when the crab is >1,200 from the patch, when rock holds it for 240 ticks, or when the smell is gone |
| Attack [nibbling] | - | In contact. A nibble every 60 ticks. Leaves when full (300 fed) or when out of reach |
| Recover [feeding] | 2,400 | On food: 20 s, then the spot is ignored |
| Idle away from home [settling] | - | Sinks home at 140 u/s; rests 1,800 ticks (15 s) before it can rise again |
| Flee -> Gone [scattered] | 43,200 | Count below 30%: scatters; a catch lies there (a body, eaten once); another cloud gathers 6 min later |

- **Boxes:** cloud hurtbox 200 x 80 on the floor point; it follows the crab over the floor and in open water above it
  (blocked by rock and by tunnel interiors, which `enemy::step` guards).
- **Numbers:** health 240 (count = health / 2, rounded up), poise 10^6 (a cloud has no balance), nibble 1 damage
  x 2 a second, floor 25 shell, wounded below 70 shell, scent 900-1,200, leash 1,200, drift 90, tell 360.
  Real vs tuned: individual size and chemoreception are from the sources above; **every number here is tuned**.
- **Statuses:** none. A nibble is a plain `Hit` through `combat::crab_defends`; `feels` is the only place the swarm
  reacts to its result (a landed nibble feeds it 10, a blocked one 2, 300 is full).
- **Drops** (`DROPS`): `"amphipods"`, given when it is scattered by smashing (deep food; scooping a dispersed swarm).
  It eats: carrion and clams (design).
- **Co-op:** blows reach only crabs with `!remote`; the defender's device caps and judges nibbles; `feels` runs on
  whichever device hears the outcome. Nothing in `src/` changes sim state.
- **Placement:** one patch on the lower canyon floor, anchored to the "narrows lock" landmark: the whale-fall
  carcass centre is 600 units east of the lock's right edge; the swarm patch is **500 units west of the carcass**
  (the giant isopod's is 500 east). With the lock's edge at x 18,080: carcass 18,680, swarm 18,180. It is dormant
  (invisible, harmless) until drawn. Note: the coral stand (a refuge) lies ~140 units east of the patch, nearer than
  the usual 400 for residents; kept, as the lead set the patch, and a refuge next to a swarm is a fair answer.
- **Encounter design:** a micro-zone beside the whale fall. The player learns: do not bleed near food. It differs
  from every hunter in the README table: it has no tell-then-blow rhythm, nothing to parry, and the answer is
  movement and leaving things behind, not timing.

## Rig notes
One tiny amphipod part (`body`: a curved C-shape, head right, with `legs` hanging from it) drawn at its true 12.5
units, and a faint `cloud` ellipse (200 x 80). `src/creatures/amphipods.rs` instances the amphipod 120 times around the
cloud's middle with jitter from a hash of (enemy id, individual number) and the sim tick; positions are
presentation only. Poses: dormant (nothing drawn), rising (a few at first, lifting out of the floor), drifting and
nibbling (the full swirl, all facing the way it is going), feeding (a tight heap over the food), settling (sinking),
scattered (14 dead ones lying on the floor). No filters, overlays, shake or bubbles.

## Scenario tests (`crates/sim/tests/amphipod_swarm.rs`)
Placement from the landmark; dormant and unhurt never draws it; a wounded crab draws it after >= 360 ticks of rise
with a `Cue::Tell` and no damage during the rise; chip rate; shell never below 25 after 90 s; block x0.2 and no
parry; no stun; a dodge through a nibble; walking away beats it; leash and give-up; refuge safety; never inside a
tunnel; food draws an unhurt crab's swarm and it feeds 20 s; food beats a wounded crab; rock makes it give up;
smashing scatters it and leaves a catch; regrowth; determinism (hash every 60 ticks); co-op (the defender judges,
the host hears the outcome through `feels`); a remote crab is never changed.

## Sources
- [W-EG] Wikipedia, "Eurythenes gryllus" (start point; cites Havermans et al. 2013, *PLOS ONE* 8(9): e74218, not
  opened). <https://en.wikipedia.org/wiki/Eurythenes_gryllus>
- [SLB-EG] SeaLifeBase, *Eurythenes gryllus*. <https://www.sealifebase.se/summary/Eurythenes-gryllus.html>
