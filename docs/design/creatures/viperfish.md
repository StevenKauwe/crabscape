# Viperfish (*Chauliodus sloani*)

Issue #577. Shared rules: `README.md`. Rig: `art/creatures/viperfish/viperfish-parts.svg`.
Register: **open water** (midnight midwater ambusher). **The one thing it teaches: your lantern is bait.** Dim it and
it loses you.

## Biology that matters
- **Size:** at most 35 cm standard length [FB-CS]; "very slender", max 35 cm [ADW-CS]. Game adult: 25 cm = 156 units,
  drawn and hit at **160 x 30** (depth about 1/5 of length: the depth ratio is a design reading, not verified).
- **Where:** bathypelagic, usually 494 to 1,000 m on FishBase, 1,000 to 2,000 m on ADW; some individuals migrate
  upward at night, an "asynchronous" vertical migration where larger ones live deeper [FB-CS][ADW-CS]. Placed in the
  midnight zone over the vent field.
- **Diet:** midwater fishes (mainly lanternfishes) and crustaceans [FB-CS]; bigger ones almost only fish, smaller
  ones more arthropods; prey up to 63% of its own length [ADW-CS]. A crab as prey: **not verified** (design
  extrapolation, like the triggerfish's jet).
- **How it hunts:** believed to wait at the depths where other vertical migrators pass and strike as prey rises to
  feed [ADW-CS]. Hence "hangs motionless, then lunges" (the waiting is sourced; the speed of the lunge is tuned).
- **The lure:** the elongated first dorsal ray arches over the head and in front of the mouth to attract prey; its
  photophores may help lure too [ADW-CS].
- **Photophores:** rows of 24 or more light organs along the sides and underside [ADW-CS]. Their use for
  counter-illumination is **not verified** on the opened pages (ADW says communication, possibly mate attraction);
  the rig draws them as lit dots on the belly and nothing in the rules depends on the reason.
- **Teeth and jaw:** the lower jaw projects past the upper, both lined with fang-like teeth; the hinged fangs rotate
  inward to keep prey from escaping; a skull-to-backbone hinge unique to *Chauliodus* lifts the skull to move big prey
  toward the throat; large gape [ADW-CS]. So: the jaw drops and the head tips back. How far and how fast the head
  tilts is **not verified** (tuned for the tell to read).
- **Predators:** dolphinfish, some dolphins, blackmouth catshark, orange roughy and others [ADW-CS]: a fish that
  flees when hurt is believable; hiding in a refuge from it is a game rule.
- **Light as bait for it:** the fish is drawn to light because its prey's light (and its own lure) is how it hunts
  in the dark: a design reading of the lure/prey behaviour above, not a measured response to a crab's lantern.

## Combat design
**Role: small ambusher, the midnight's small terror.** Several dim lights in the dark and one is a mouth.
- **Senses (the lesson):** it notices by light. The crab's lantern at full glow is noticed from **800 units**
  (+100 per culture in the lantern, up to 4); a **dimmed** lantern only from **160**. Once it has a crab, if the crab
  stays beyond its light's reach for **1 s (120 ticks)** it loses it ("lost"): it drifts home at 70 units/s and is
  deaf to light for 2 s more. Light source used: `Mover::lantern_dim` (the shell action that dims the lantern, it
  exists on main) and `Mover::lantern_glow` (cultures). Blinded crabs (ink) are lost at once.
- **Attacks:** one, the **lunge bite**: it closes to 120 units, pulls its head back and drops its jaw, then darts
  along the aim locked when the lure went dark.
- **Tell: 1.2 s (144 ticks), a lunge ambush, not a long hunter's tell** (it is slow and small; its blow is 14, far
  from lethal, and the brief's open-water 3 s rule is for big hunters). Head tips back, jaw opens, fangs show, lure
  twitches; **the lure tip goes dark for the last 0.4 s (48 ticks)** and the aim locks then. It tracks the crab
  until then, so stepping aside after the dark lure dodges the locked lunge.
- **The crab's answers:** *dim the lantern* before it closes (it loses you); *dodge* (needs a walk direction, press on
  the dark lure: invulnerable ticks 2 to 13, the bite lands about 4 ticks into the lunge); *parry* (window 10 ticks,
  staggers it 90 ticks, counter x2); *block* (x0.2 damage, 20 poise); step out of the locked aim; *shelter* (it breaks
  off from a crab in a Refuge/Mouth/Tunnel or holding coral or kelp, and never enters tunnels).
- **Weaknesses:** open-jawed recovery (hits to the front 70 units while it lasts count x1.5); poise 25 breaks on one
  smash; light health 35.
- **Animals want to eat:** after 3 lunges in one engagement it has had enough and drifts home; hurt to half it flees
  once (700 units away, 2 s) and hides 3 s before returning; it mends at home after 10 s unhit.

## Implementation spec (`crates/sim/src/creatures/viperfish.rs`)
| State | Ticks | Notes |
|---|---|---|
| Idle (hang) | - | Motionless at home; senses crabs by light. Drifts home at 70 u/s when displaced |
| Notice | 24 | Turns to the crab. After a lunge, a random 50 to 100 tick pause (seeded rng) |
| Approach (drift) | - | Swims at 110 u/s straight at the crab (2D, stops at rock); tell when its mouth is within 120 units with a clear line |
| Tell (gape / dark) | 144 (dark: last 48) | Aim tracks the crab until tick 96, then locked. No damage in the tell |
| Attack (lunge) | 18 | Whole body darts 680 u/s along the aim (about 100 units); bite circle r 26 at the mouth; lands about tick 4 |
| Recover (open) | 120 | Hangs open-jawed; front hits x1.5 |
| Stagger | 50 (parried 90) | Poise broken, or parried |
| Flee | up to 240 | Once, below 50% health; then hides 360 ticks |
| Lost | 240 | Drifting home, deaf to light |

- **Boxes:** body hurtbox 160 x 30; bite circle r 26 at the mouth (front of the head, mid-body), swept as it lunges.
- **Numbers:** health 35, poise 25 (refill 20/s after 60 ticks, stagger 50). Bite **14 damage, 20 poise**, knockback
  160, hit-stop 6, parryable and blockable, **no venom**. No single blow or undefended run kills a full crab: 3 lunges
  at most per engagement, 14 each = 42 of 100.
- **Real vs tuned:** length, jaw, lure and photophore anatomy are sourced; every timing, range and damage is tuned.
- **Statuses:** none applied (no venom, no grab).
- **Drops (`DROPS`):** `viperfish fang`, `photophore`. It leaves its body to be eaten like the others.
- **Eats:** lanternfish and crustaceans (not modelled).

## Where it lives and the encounter
- **Placement:** midwater over the **"vent field"** landmark (lower canyon, open water): two fish, at 420 and 700 units
  above the floor, 30% and 70% across the field, anchored by landmark name and `floor_height`. Both at least 400 units
  from the field's coral-stand refuges, outside tunnels. No phases: it is a wandering ambush, not an arena.
- **What the player learns:** the lantern is a lure for things as well as a light. In the lower canyon the crab can
  dim it to slip past; walking bright from stand to stand is paid for in tells. Dim lights in the dark, one is a mouth.
- **Versus the others:** the moray ambushes from a crevice by smell and holds on; the triggerfish uses a water jet;
  the jelly is a hazard. The viperfish is the only one that reads the crab's light and the only one you can
  escape by changing the light.

## Rig notes
Parts: `body` (root), `tail`, `head` (pivot at the neck, tips back), `jaw` (hinge under the head; own lower fangs),
`head-fang-n` (upper), `lure` (the first dorsal ray, a line arching forward over the head, pivot at its base) with
`lure-tip` (a glowing dot, own part: hidden in the tell's dark phase), `eye`, belly photophores (`body-photo-n`).
Poses (all read the sim): **idle** lure sways; **move** tail beats faster; **tell** head back up to 0.32 rad, jaw down
to 0.95 rad, lure twitches, tip dark for the last 0.4 s, body turns to the aim; **strike** jaw wide then shutting in the
last third while the whole fish darts; **recover** open-jawed; **hurt/stagger** jaw wobble; **defeat** belly up, jaw
slack, hidden once eaten.

## Scenario tests (`crates/sim/tests/viperfish.rs`)
Placement over the vent field, at two depths, 400+ from shelters; a bright lantern is noticed from further than a dim
one (and cultures add reach); dimming makes it lose you after about 1 s and it drifts home deaf to light; the tell
lasts 144 ticks with the lure dark for the last 48 and a `Cue::Tell`; an unanswered lunge bites once for 14, no
venom, then 120 ticks open-jawed; parry; block (x0.2); dodge; the crab's smash breaks its poise; it flees at half and
a second bout kills it; escape by stepping out of the locked aim and by refuge; gives up after 3 lunges; no blow or
undefended run kills a full crab; never enters a tunnel; determinism (hash every 60 ticks, 20 s); co-op two-sim
(the defender's device judges the bite once, both devices agree); a remote crab is never changed by the fish.

## Sources (opened 2026-11-11)
- [FB-CS] FishBase, *Chauliodus sloani*. <https://www.fishbase.se/summary/Chauliodus-sloani.html> (length, depth, diet, diel migration; no lure, teeth or jaw text)
- [ADW-CS] Animal Diversity Web, *Chauliodus sloani*. <https://animaldiversity.org/accounts/Chauliodus_sloani/> (size, depth, migration, diet, lure, photophores, jaw and teeth, predators)
