# Queen triggerfish (*Balistes vetula*)

Concept sheet: `art/creatures/queen-triggerfish/queen-triggerfish.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** up to 60 cm, commonly 30 cm [FB-BV]. Game adult: 45 cm (281 units), body depth ~55% of length.
- **Where:** rocky and coral areas, 2–275 m, usually 3–30 m; schools or alone over sand and seagrass [FB-BV]. Reef and
  the sand around it.
- **Diet:** mainly bottom invertebrates [FB-BV]. It preys on sea urchins by **blowing water to overturn them**, then
  attacks where the spines are short [FB-BV]. Crabs of our size as prey: not verified for this species (other
  triggerfish eat crabs, e.g. the orange-lined triggerfish: aquarium-trade source only).
- **Jaws:** small, strong, with teeth for crushing shells (Wikipedia, citing Turingan & Wainwright 1993).
- **Armour:** tough skin of rough, plate-like scales (Wikipedia, uncited there: not verified).
- **The trigger:** the first dorsal spine locks upright when the short second spine slots into a groove at its base;
  pressing the second spine down releases it. Young triggerfish wedge into crevices with the locked spine and lowered
  pelvic bone [AM].
- **Swimming:** slow cruising by rippling the soft dorsal and anal fins; the tail for fast bursts (Wikipedia, citing a
  reef-keeping site: not verified).

## Combat design
**Role: tank + ranged.** A big armoured fish that flips you with water, then bites the soft side. Teaches bracing and
finding weak spots.
- **Attacks:** *jet* (a blast of water in a cone ahead of the mouth that flips the crab onto its back: the urchin trick
  turned on a crab, a design extrapolation of a real behaviour); *bite* (on a flipped crab, or close in); *hide*
  (wedges into a crevice and locks its spine when badly hurt).
- **Tell:** it turns nose-down and hovers over you, first spine raised, mouth pursed (head-down hovering before the jet
  is a design reading, not verified). Bite tell: closing in with the mouth open.
- **The crab's answers:** *block* = brace: claws dug in, not flipped, pushed back; *dodge* out of the cone; *parry* the
  bite (punishes the mouth). If flipped, mash to right yourself before the bite. When it hides, strike the small
  second spine (the trigger) to unlock it and pull it out: the real release.
- **Weaknesses:** the mouth and eyes (unarmoured); a long recovery after a jet; it can't jet while turning.
- **Feel:** cozy: it pokes about the sand like a gardener; brutal: on your back, legs waving, watching it turn;
  surreal: a fish that uses water as a tool.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Forage | — | Cruises 120 u/s over sand, blows at the floor (harmless puffs that uncover food). Notices the crab by sight at 360 units, by day (night: rests in a crevice; not verified) |
| Approach | — | Bursts at 400 u/s to above-and-ahead of the crab |
| Tell: nose-down hover | 72 | Tuned |
| Jet (active) | 12 | Cone from the mouth, 120 units long, half-angle 20°, aimed at the floor |
| Bite tell | 18 | Mouth opens on the approach |
| Bite (active) | 6 | Parry window 10 ticks |
| Recover | 48 | After a jet; mouth and eye hits ×2 |
| Stagger (parried) | 50 | |
| Hide | — | Below 25%: wedges into the nearest crevice, spine locked: no damage until the trigger spine is hit, then 90 ticks stunned |

- **Boxes:** body hurtbox a diamond 281 × 155; mouth hurtbox circle r 16; trigger-spine hurtbox (only while hiding)
  circle r 8. Flank hits ×0.5 (armour).
- **Numbers:** health 150, poise 120. Jet 0 damage, *flipped* 120 ticks (each dodge press −10); braced: pushed back 40
  units, 30 poise. Bite 20 damage (×2 on a flipped crab), 40 poise.
- **Drops:** scale plates (armour material: the "reef armour" of `world.md`) and food.
- **Scenario tests:** a braced crab in the cone isn't flipped; an unbraced one is flipped for 120 ticks; mashing rights
  it sooner; a flipped crab takes ×2 from the bite; a parry in the bite window cancels it and opens the counter;
  flank hits deal half; while hiding only a hit on the trigger spine deals damage, and it releases the lock.

## Rig notes
Parts: body (one rigid diamond), mouth (small jaw, opens/purses), eye (moves within its socket), first dorsal spine
group (big spine, membrane, trigger spine; pivot at the base), soft dorsal fin and anal fin (each as 3–4 strips that
ripple in sequence), caudal fin (lunate, with top and bottom streamers that lag), pectoral fin, pelvic spine.
Two blue face lines and the scale lattice are painted on the body.
Key poses: **idle** (dorsal and anal fins ripple in a travelling wave, pectoral flutters, tail folded); **tell** (whole
body rotated −30°, spine up, mouth pursed); **jet** (mouth open, a water cone sprite, small recoil back);
**recover** (levels out, fins flare); **hurt** (tail burst away, spine snaps up).

## Sources
- [FB-BV] FishBase, *Balistes vetula*. <https://www.fishbase.se/summary/Balistes-vetula.html>
- [AM] Australian Museum, "Why are they called triggerfishes?". <https://australian.museum/learn/animals/fishes/why-are-they-called-triggerfishes/>
- Wikipedia, "Triggerfish" (start point). <https://en.wikipedia.org/wiki/Triggerfish>
