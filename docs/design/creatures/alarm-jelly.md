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

## First playable slice (2026-11-04)

One optional jelly drifts above the eastern Silt-to-Coral return at (15180, -2830).
The lower route passes below its 420-unit trailing tentacle, including a grown
crab's downhill return. Existing coral and upper shelves provide an approach to
the bell; no extra stroke, pickup, terrain or progression gate is added.

A bell bump or ordinary claw strike starts a three-second blue pinwheel, then a
five-second refractory period. The nearest eligible moray within 900 units and
800 units of its own refuge investigates the alarm's original position along a
clear, bounded route. It returns to ordinary AI after three seconds. Committed
tells, bites, grabs, fleeing and hiding are not interrupted. This is a deliberately
tuned reuse of an existing hunter, **not** a claim that snowflake morays are
natural Atolla predators, and it does not implement enemies eating a pursuer.

Tentacle contact applies the existing 2 damage/second venom for two seconds.
Guard blocks it; a successfully dodged brush consumes the same two-second contact
window, preventing a persistent tentacle from immediately hitting again after the
dodge ends. Each crab judges its own damage. Bell hit messages and the existing
Enemy snapshots carry the alarm; the hunter uses its existing timed Approach and
swim goal, with no additional saved fields. No new unbounded search or population.

The 94-unit red bell, coronal groove, marginal fringe and six visible trailing
links use the existing Rust rig/oil pipeline. Collision and animation share the
trailing polyline; blue rim points and an expanding ring make the alarm distinct.
The bell uses the usual lantern lighting; this slice adds no blinding status.

Scope deliberately deferred: the proposed 10-health kill/hurt/death behavior,
harvesting, enemy-on-enemy alarm predation, and a general light-sensing system.
This first environmental jelly responds to hits without health loss and cannot be
crusher-held or eaten. The implemented timings, sting severity, bounded drift,
420-unit tail and habitat spacing are game choices. MBARI's opened account supports
red camouflage, midwater habitat, a long tentacle in most Atolla, and a blue alarm
that attracts larger predators; it does not establish crab-sized sting severity or
this particular hunter's response. Species is left at *Atolla* spp., not the
long-tentacle-lacking *A. reynoldsi*.

Existing authoritative saved enemy lists remain authoritative: this appended
habitat appears in fresh populated worlds, and is not injected into an already
saved roster. No reset or world migration is performed. Prior enemy IDs and enum
variants retain their order.

Headless demonstration (ordinary approach with guard, then leave below):

```sh
cargo crabsim --start 15080,-2850 --seconds 4 --every 30 \
  --script 't=0 spawn=alarm-jelly@15180,-2830 spawn=moray@15220,-3460 walk=right block' \
  --dump /tmp/alarm-jelly-demo.jsonl
```

The focused cellular scenarios additionally prove normal/max safe lower return,
guarded bell approach drawing the actual local hunter upward, bounded expiry,
nearest/range/obstruction behavior, serialized continuation and guest-hit/host
investigation with defender-owned sting effects.
