# Giant isopod (*Bathynomus giganteus*)

Rig: `crates/crabrig/src/bestiary/isopod.rs` (placeholder ink art). Shared rules: `README.md`. Issue #582.

## Biology that matters
- **Size:** *B. giganteus* typically 19–36 cm, the largest confirmed about 50 cm [WIKI-GI, citing Lowry & Dempsey 2006
  and McClain et al. 2015; not opened directly]. Game: 30 cm = 190 units, hurtbox 190 x 50 (flattened shell with legs under it).
- **Where:** deep soft sea floor; most giant isopods live about 170–2,140 m, over 80% of *B. giganteus* between 365
  and 730 m [WIKI-GI]. Game: the lower canyon floor, far deeper than the real peak: a game choice so it meets the
  whale fall (not verified as a real depth for this species).
- **Diet:** a scavenger of dead whales, fish and squid, and it may take slow prey; fish were the commonest food in a
  study of 1,651 animals [WIKI-GI, citing Briones-Fourzan & Lozano-Alvarez 1991]. Food is scarce, so it gorges on a big
  carcass until it can hardly move [WIKI-GI]. Whale falls are stripped by scavengers within days to months [MBARI]; MBARI's
  page does not name isopods (not verified that isopods feed at whale falls).
- **Fasting:** captives have gone over five years without food [WIKI-GI, citing NPR and the Japan Times; not opened
  directly]. Game: it need not hunt; it guards a food spot and does not chase far.
- **Body:** large compound eyes (nearly 4,000 facets), set far apart; first leg pair modified into maxillipeds that bring
  food to four sets of jaws; seven pairs of walking legs; five pairs of swimming/gill pleopods under the tail [WIKI-GI].
  Hard dorsal shell (tergites) over a softer underside.
- **Defence:** it can roll into a ball like a woodlouse, showing only the hard dorsal shell [WIKI-GI]. Whether it
  bites or shoves a rival at a carcass is **not verified**: the bite (mouthparts) and the butt are design extrapolations.

## Combat design
**Role: scavenger at a food spot, a tank with a soft belly.** One lesson: **armour is on top**: wait for it to rear and
hit the belly, or turn it over. Register: micro-zone scavenger (not a hunter: it breaks off).
- **Attacks (they alternate, shove first, so the pattern is learnable):**
  - *Shove:* it lowers its head and braces (tell 0.6 s), then butts forward 36 units: **no damage**, 45 poise, a push of
    360 u/s (shoves a crab off its food). Not parryable (the whole shield), blockable, dodgeable, missed if you back away.
  - *Rear-and-bite:* it tips its front up on the hind legs, pereopods raised and mouthparts working, belly bared (tell
    0.9 s), snaps down (active 6 ticks): 14 damage, 30 poise, parryable, blockable.
- **Armour:** every blow on its shell does x0.25 (a smash does 3, a jab 1.25). **Belly x2** while reared (the first half of
  the bite's tell, then the cocked second half covers it again) and while it settles (the first 72 ticks of the bite's
  recovery). **Flipped x2.** **Rolled x0.1.**
- **The crab's answers:** *parry* the bite at the end of its tell: no damage and it is **flipped** (2.5 s, x2 everything)
  and then rights itself (0.4 s). *Poise break* also flips it (poise 60, smash 25: three smashes). *Hit the belly* on the
  rear or settle. *Block* the shove (poise still drains), *dodge* it, or walk away (it is slow). *Lantern:* it sees; a
  dimmed lantern shrinks its notice (the generic sense).
- **Weaknesses:** slow (60 u/s ambling, 90 u/s closing in), a fixed tell, the bared belly, a flip.
- **Flees:** hurt to 30% (27 health) it curls into a half-ball (3 s, x0.1), then crawls away from the crab (5 s, at most
  260 from its feeding ground) and afterwards only reacts to a crab within 110 units; unhurt and alone 20 s it mends.
- **Fairness:** a tell before every blow (72 and 108 ticks, from rearing/bracing); max one blow per 4+ s; no blow kills a
  full crab (14); two undefended bites do 28. It breaks off a crab beyond 450 from its feeding ground, a crab in the
  home shell, a crevice door, a tunnel or gripping kelp/coral. It never enters a tunnel (the shared sanctuary guard).
- **Crab's-eye feel:** cozy: a slow grey neighbour grazing on the whale; brutal: a stone plate that shrugs off your
  best smash; surreal: a woodlouse the size of a loaf, with eyes like beads, rearing up over you.

## Implementation spec (`crates/sim/src/creatures/isopod.rs`; feel numbers are named consts there)
| State (phase) | Ticks | Notes |
|---|---|---|
| Idle (graze / wander) | - | Amble up to 120 from home at 60 u/s, graze 3-6 s, amble 2-5 s (seeded). Returns home if displaced |
| Notice (alert) | 48 | Notices a crab at 250 (sight; x1.35 if the lantern is lit in the dark) |
| Approach | - | 90 u/s until the crab is within 70 of its head |
| Tell (brace) | 72 | Shove. Committed to its side (no tracking) |
| Tell (rear, cocked) | 108 | Bite. Rear = first 54 ticks (belly x2), cocked = last 54 |
| Attack (shove / bite) | 8 / 6 | Shove: circle r 26, body lunges 36. Bite: circle r 28, 10 ahead of the head, 22 high |
| Recover | 72 / 96 | After a bite, belly x2 for the first 72 |
| Stagger (flipped) | 300 | Poise break or parry; x2 |
| Recover (righting) | 48 | Then a 30-70 tick pause and it sizes the crab up again |
| Flee (rolled, crawl) | 360 + 600 | At 30% health, once until it mends |

- **Numbers:** health 90, poise 60 (refill 20/s after 90 ticks unhit), bite 14 / 30 poise, shove 0 / 45 poise.
  Real: size, belly vs shell, the roll. Tuned: every number above.
- **Armour in the sim:** the shared `Enemy::takes` has no zones, so the isopod settles each blow on its next step: the
  loss `takes` booked is rescaled by `State::armour`, the factor of the pose it was in when the blow landed. The kill check
  inside `takes` uses the raw damage (a nearly dead isopod can fall to one smash that its armour would have shrugged off);
  a death the rescale causes (belly x2) is booked by the isopod itself. A shared change would be a `creatures::takes`
  hook (damage zone by pose and attacker side) in `enemy.rs`, plus an exposure/zone field on `EnemyTuning`.
- **Statuses:** none on the crab; on itself: flipped, rolled.
- **Drops (`DROPS`):** `"isopod plate"` (a shed dorsal tergite: armour material, not verified as a real shed part), and its
  body as deep food. It eats carcasses (it is not hunted food for itself).
- **Where it lives:** the lower canyon floor, 500 east of the whale-fall carcass; the carcass centre is 600 east of the
  "narrows lock" landmark's right edge (shared with the hagfish and amphipod lanes): x 19180 from the lock edge 18080.
  Its feeding ground is the spot itself. It is about 310 from the third coral stand (x 19490+) a refuge, closer than the
  400 the common brief asks for; the lead set the offset.
- **Encounter:** a separate scavenger encounter at the carcass's edge: no tell hides a trap, walk around it or learn it. How
  it differs: it is the slow armoured tank; the mantis is fast, the triggerfish ranged, the moray an ambusher.
- **Scenario tests (`crates/sim/tests/giant_isopod.rs`):** placement; the shove's 72-tick tell, cue, no damage and push;
  the bite's 108-tick tell and 14 damage once; no undefended pair kills; a parry flips it with no damage; a dodge takes
  nothing; backing off makes it miss; back x0.25, belly x2, flipped x2, cocked covers again, settle bares again; poise
  break flips; roll-up at 30%; it gives up; a crab in a refuge is left alone; it can be beaten by a crab that parries and
  hits the belly; determinism; a co-op two-sim test; a remote crab is never changed.

## Rig notes (cut-out puppet, `crates/crabrig/src/bestiary/isopod.rs`)
17 parts at true size: `body` (pereon shield, root, pivot at the hind legs so the game can tip it), `rear-body`
(tergites 5-7), `pleotelson`, `uropod`, `head`, `eye` (with glint), `antenna`, `antennule`, `maxilliped`, a pale `belly`
plate, four near and three far `leg-*`. Poses in `src/creatures/isopod.rs`: rear (body tipped about the hind legs,
front legs raised, maxillipeds working), cocked, bite (head dips), brace, shove, flipped (body mirrored, legs wave),
righting (mirror back), rolled (rear and head curl under, legs tucked), walk (legs and feelers).

## Sources
- [WIKI-GI] Wikipedia, "Giant isopod" (start point; its facts cite Lowry & Dempsey 2006, McClain et al. 2015, Briones-Fourzan & Lozano-Alvarez 1991, NPR, Japan Times: not opened directly). <https://en.wikipedia.org/wiki/Giant_isopod>
- [MBARI] MBARI, whale falls (stages and scavengers; does not name isopods). <https://www.mbari.org/news/whale-falls/>
- Animal Diversity Web and SeaLifeBase/FishBase pages for *B. giganteus* returned no usable content (taxonomy only, a 403, "not in database"): not verified from them.
