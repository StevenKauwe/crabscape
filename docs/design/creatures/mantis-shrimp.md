# Peacock mantis shrimp (*Odontodactylus scyllarus*)

Concept sheet: `art/creatures/mantis-shrimp/mantis-shrimp.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** 3–18 cm long [ADW], so an adult is about as long as our crab is wide, but slim. Game: 15 cm = 94 units.
- **Where:** Indo-Pacific reefs, 3–40 m, in U-shaped burrows in sand and rubble near coral bases [ADW]. Shallows/reef edge.
- **Diet:** snails, crustaceans and bivalves; **crabs are its preferred prey** [ADW]. It strikes the shell again and
  again until it can reach the soft parts (Wikipedia, citing Caldwell).
- **The strike:** the raptorial club is cocked and latched while muscles load a saddle-shaped spring, then released
  [UCB]. Peak speed 14–23 m/s, accelerations 6,300–8,000 g [UCB]; strikes last a few milliseconds and hit with
  400–1,501 N [PC05]. Each strike lands **twice**: the club, then a collapsing cavitation bubble 390–480 µs later that
  adds up to 504 N [PC05].
- **Senses:** stalked compound eyes that see colour, ultraviolet and polarised light [ADW]. A sight hunter.
- **Defence:** retreats to its burrow; fights rivals with blows to the heavily armoured telson; produces low "rumbles"
  to warn off predators and rivals [ADW]. The **meral spread** (raptorial arms spread wide, showing the meral spots) is
  a threat display used in fights [FR16]. Freshly moulted mantis shrimp bluff with it and flee if challenged
  (Steger & Caldwell 1983, read via a bibliography summary: not verified).

## Combat design
**Role: rusher and armour breaker.** A fast, dangerous duel that teaches the parry.
- **Attacks:** *club strike* (repeats up to 3 times if the first lands, re-cocking between, as it does on shells);
  *dart* (a quick scuttle to close distance).
- **Tell:** a rumble and a rear-up (front lifts ~7°), then the club pulled back tight under the head: the latch is set.
  That hold-still beat is the warning. Each re-cock in a combo shows the same pull-back, shorter.
- **The crab's answers:** *parry* at the end of the hold: the claw meets the club's side before impact, so neither the
  club nor the bubble lands, and the shrimp staggers. *Block*: the club is stopped but the bubble still pops against
  the claws (chip damage, armour cracked). *Dodge* through it. *Lantern* doesn't matter (it hunts by sight in light;
  in the dark it notices you later).
- **Weaknesses (from biology):** it must come out of the burrow to strike; long recovery after a miss with the head low;
  only the telson is heavily armoured.
- **Crab's-eye feel:** cozy: a jewel-coloured neighbour with swivelling eyes; brutal: the loudest crack on the reef,
  aimed at you; surreal: a punch faster than a bullet's start from something your size.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Burrowed | — | Only eyes show; can't be hit. Notices the crab by sight at 300 units in light, 150 in the dark (tuned) |
| Emerge | 24 | Leaves the burrow |
| Approach | — | Darts at 330 u/s (tuned: faster than the crab's walk of 270, slower than its sprint of 480) to strike range 70; chases up to 800 from its burrow |
| Tell: rumble + rear | 48 | Tuned (real wind-up not measured). It stalks: turns after a crab that slips past and closes in on one that backs out of reach, so walking away doesn't dodge it (not verified) |
| Tell: cocked hold | 30 | Parry window = its last 10 ticks. Committed to its side, still closing in |
| Strike (active) | 3 | Tuned (real ≈ 3 ms, under one tick) |
| Bubble hit | +1 | 1 tick after impact (real 0.4–0.5 ms) |
| Re-cock (combo) | 24 | 1 more strike if the first connected (capped at two strikes for now) |
| Recover | 60 | Head low: head hits ×1.5 |
| Stagger (parried) | 50 | |
| Flee | — | Once a fight, below 25% health, back into the burrow; peeks out again after 2.5 s |

- **Boxes:** body hurtbox 94 × 22 centred on the body; club hitbox a circle r 12 at 40 units ahead of the head.
  Telson hurtbox takes ×0.5.
- **Numbers:** health 60, poise 50. Club 28 damage, 60 poise, and *armour break*: the crab's block takes ×0.4 instead
  of ×0.2 for 5 s. Bubble 8 damage, ignores block, cancelled by parry.
- **Drops:** a club fragment (hammer-branch material: the club's mineralised surface resists fracture, Wikipedia citing
  Weaver et al. 2012) and food.
- **In the sim:** `Kind::Mantis` (`crates/sim/src/enemy.rs`, numbers in `tuning.rs`, tests in
  `tests/scenarios.rs`). Tuned beyond this table: head zone 24 and telson zone 16 units, poise refill 25/s, poise-break
  stagger 50, gives up 500 from its burrow, a 20–50-tick random pause between attacks, club knockback 300 and 12 ticks
  of hit-stop; a blocked club does nothing until the guard is cracked (then ×0.4). Another mantis moves into an empty
  burrow 20 s after one dies. Home with no crab near and unhit for 2 s, it mends (full health, cracks gone, may flee again). An undefended full combo (2 × club + bubble = 72) hurts badly but can't kill a full shell (capped at two
  strikes until located body damage brings the brutality, PR #90).
- **Scenario tests:** parry in the window takes no damage from club or bubble; block takes only the bubble (8 × 1) and
  sets armour break; a dodge through the strike takes nothing; a burrowed mantis can't be hit; a 2-strike combo only
  follows a landed first strike; a missed strike gives the full 60-tick recover.

## Rig notes (cut-out puppet)
Parts: 2 eyestalks + eyes (each rotates on its own), 2 antennules (3 flagella each), 2 antennal scales, carapace,
3 exposed thoracic segments, 6 abdominal segments (a chain, pivots at each front edge), telson, 2 uropods, raptorial
appendage ×2 (merus; propodus + folded dactyl as one "club" part), 3 pairs of walking legs, 2 small maxillipeds, a
pleopod fringe. Far-side limbs are darker copies behind the body.
Pivots: raptorial shoulder under the front of the carapace; club at the merus tip; walking legs at their bases.
Key poses: **idle** (eyes swivel independently, antennal scales flick); **tell** (body rotated −7° about the tail,
merus swung back, club tight); **strike** (club out flat ahead, one smear frame, bubble flash at the tip);
**recover** (head down, club drooping); **hurt** (abdomen curls, telson raised toward the attacker).

**Puppet (in the game):** drawn by `crates/crabrig/src/mantis.rs` from the concept sheet (side view, facing right) into
`art/creatures/mantis-shrimp/mantis-shrimp-parts.svg`, exported to `assets/creatures/` (`cargo run -p crabrig --profile
art -- parts --creature mantis-shrimp`, then `-- export --creature mantis-shrimp`). Sixteen parts: carapace (with the
thorax, near walking legs and mouthparts), abdomen (six segments and the swimmerets' fringe), tail fan (telson and
uropods), and near and far: raptorial arm, club, eye on its stalk, antennule, antennal scale; three far walking legs.
The game (`src/foes.rs`) rears the body about its hind feet (the abdomen turns back so the tail stays down), swings the
arm and club through folded, cocked, out and drooping, swivels each eye on its own, sweeps the antennules and flicks
the scales, curls the abdomen and lifts the tail fan when hit, and sinks it in the burrow so only the eyes and
antennule tips show. The strike adds a short lunge and stretches the club for a frame so it reaches toward the hit
circle. The club is drawn bigger and redder than life, with a pale gleam while the latch is set, so the tell reads at
phone size. Ink style only for now (no oil-painted copy yet); the rig notes' separate abdominal segments, maxilliped
pair and meral spread are not separate parts yet.

## Sources
- [ADW] Animal Diversity Web, *Odontodactylus scyllarus*. <https://animaldiversity.org/accounts/Odontodactylus_scyllarus/>
- [UCB] UC Berkeley news release (2004), "Mantis shrimp may have swiftest kick". <https://newsarchive.berkeley.edu/news/media/releases/2004/04/21_shrimp.shtml>
- [PC05] Patek & Caldwell (2005), "Extreme impact and cavitation forces of a biological hammer", *J. Exp. Biol.* 208:3655–3664. <https://escholarship.org/uc/item/9938507n>
- [FR16] Franklin, Marshall & Lewis (2016), "Multimodal signals: ultraviolet reflectance and chemical cues in stomatopod agonistic encounters", *R. Soc. Open Sci.* 3:160329. <https://pmc.ncbi.nlm.nih.gov/articles/PMC5108963>
- Wikipedia, "Odontodactylus scyllarus" (start point). <https://en.wikipedia.org/wiki/Odontodactylus_scyllarus>
