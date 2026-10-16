# Alarm jelly (deep-sea crown jelly, *Atolla*)

Concept sheet: `art/creatures/alarm-jelly/alarm-jelly.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** bell up to ~15 cm [MBARI]; *A. wyvillei* 2–17 cm [WP-AW]. Most have one long trailing tentacle up to six
  times the bell's width [MBARI]. Game: bell 94 units, trailing tentacle up to ~560.
- **Where:** midwater, 500–5,000 m, mostly the midnight zone [MBARI].
- **Look:** a bright red bell, which reads as black in the deep [MBARI]; about 20 marginal tentacles plus the one long
  one [WP-AW]; a deep coronal groove around the dome.
- **Diet:** plankton, including crustaceans, snagged by the trailing tentacle [MBARI]. A crab our size is not its prey.
- **The alarm:** when attacked it throws "pinwheels of brilliant blue bioluminescence" that disorient the attacker and
  draw bigger predators, which drive off the first one [MBARI]. Flashes travel in circular waves at 5–50 cm/s [WP-AW].
  Edith Widder's "e-jelly", a ring of 16 blue LEDs copying it, lured a giant squid on camera (NOAA via search: not
  opened, not verified).
- **Sting:** scyphozoan jellies sting with nematocysts; how badly *Atolla* stings something crab-sized is not verified.

## Combat design
**Role: environmental hazard.** Weather, not an enemy. It never attacks; the danger is touching it, and worse,
alarming it.
- **Tell:** none, honestly: out of the lantern's light it's a black shape on black. What gives it away is the drifting
  tentacle line and the slow pulse. Your yellow lantern shows its red.
- **Touch:** brushing a tentacle stings (venom). Raised claws block it.
- **Alarm:** hitting it or bumping the bell sets off the pinwheel: for 3 s the nearest big hunter in range is drawn to
  the spot. Light travels outward slowly enough to see.
- **Using it:** the same rule works for the crab: lure a hunter chasing you onto a jelly and alarm it, so the bigger
  thing turns on your pursuer.
- **Feel:** cozy: drifting red lanterns, quietly beautiful; brutal: one clumsy step and something enormous turns toward
  you; surreal: an animal that screams in light.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Drift | — | Moves with a slow current (12 u/s) and a pulse every 120 ticks that lifts it 20 units; trailing tentacle sways (a 6-point chain) |
| Sting (on contact) | — | Tentacle touches the crab: *venom* 2 damage/s for 2 s; blocked if the claws are up |
| Alarm | 360 | Triggered by any hit or a bell bump: emits an `Alarm { pos, radius: 900 }` event; hunters within the radius switch to Approach toward `pos` |
| Recover | 600 | Can't alarm again for 5 s (tuned) |

- **Boxes:** bell hurtbox ellipse 94 × 40 (any hit alarms; health 10, it pulses away when hit); tentacle sting boxes:
  marginal ring r 50 under the bell, trailing tentacle as 6 circles r 6.
- **Visibility:** draw it at full red only inside the lantern's light; outside, near-black with a faint rim (matches the
  game's lit/unlit blend).
- **Drops:** none.
- **Scenario tests:** walking into a tentacle with claws up takes no venom; with claws down takes 4 over 2 s; a crusher
  hit on the bell emits exactly one alarm event; a second hit within 600 ticks emits none; a hunter inside the alarm
  radius moves toward the alarm spot, one outside doesn't.

## Rig notes
Parts: dome, bell (lappet rim as one part that flexes), stomach (seen through the dome), ~10 near-side marginal
tentacles (each 2 segments, curl), trailing tentacle (8-segment chain), alarm ring (a separate glow sprite:
blue rings that travel outward along the rim) and 7 light points on the rim.
Key poses: **drift** (bell open, tentacles curled outward); **pulse** (bell contracts 15%, tentacles trail);
**alarm** (rim points flash in sequence like a pinwheel, rings expand); **hurt** (bell clenches, drifts away).

## Sources
- [MBARI] MBARI, "Deep-sea crown jelly". <https://www.mbari.org/animal/deep-sea-crown-jelly/>
- [WP-AW] Wikipedia, "Atolla wyvillei" (start point). <https://en.wikipedia.org/wiki/Atolla_wyvillei>
