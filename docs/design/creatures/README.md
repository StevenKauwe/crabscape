# Bestiary

The first enemies, one page each, with a concept sheet in `art/creatures/<name>/` (SVG source + painted PNG) and all
of them on `art/creatures/bestiary-contact-sheet.png`. World and food web: `../universe.md`.

| Creature | Role | Zone | The tell (real behaviour) |
|---|---|---|---|
| [Snapping shrimp](snapping-shrimp.md) | Turret, short-range shock | Shallows rubble | Snapper claw cocked open (~0.5 s) |
| [Mantis shrimp](mantis-shrimp.md) | Rusher, armour breaker | Shallow reef burrows | Rumbles, rears, cocks its club |
| [Snowflake moray](snowflake-moray.md) | Ambusher | Reef crevices | Pulls back into an S, mouth shuts |
| [Queen triggerfish](queen-triggerfish.md) | Tank + ranged | Reef and sand | Hovers nose-down over you, spine up |
| [Common octopus](common-octopus.md) | Boss | Rocky shallows, its den | Rises and spreads its web |
| [Alarm jelly](alarm-jelly.md) | Hazard | Midnight zone | None: a red shape on black and a drifting tentacle |

## Shared combat model (to build in `crates/sim`; every number tuned, lives in `Tuning`)
Times are ticks at 120 Hz. Sizes in world units (1 cm = 6.25). "Real" = measured in a source; "tuned" = chosen for play.

**The crab**
| Thing | Spec |
|---|---|
| Health ("shell") | 100. Zero = death (moult left behind, `world.md`) |
| Poise | 100; each hit's poise damage drains it; at 0 the crab staggers 40 ticks. Refills 40/s after 60 ticks unhit |
| Hurtbox | The body box (`body_half_width` 36, `body_height` 48) |
| Crusher smash | Windup 14, active 6, recovery 22. Damage 12, poise 25. Hitbox 40 × 30 beside the body on the facing side |
| Cutter jab | Windup 6, active 4, recovery 12. Damage 5, poise 8 (scissor claws: ×2 vs soft parts and arms) |
| Block (hold) | Claws up: physical damage ×0.2, poise damage still taken; stings and bites that hit the guard do no venom. Walk at 40% |
| Parry | Block pressed ≤ 10 ticks before a hit lands: no damage, attacker staggered (its own number), then a 45-tick counter window where the crusher does ×2 |
| Dodge | The existing dodge (18 ticks); invulnerable on ticks 2–13 |
| Autotomy | While grabbed: down + dodge drops a walking leg and frees the crab at once; −8% walk speed per missing leg until regrown (regrowth rules: open task) |

**Statuses** (each one a real mechanism): *stagger* (poise broken), *stunned* (shock wave; can't act), *venom*
(damage over time + slow; only through a bite or sting that isn't blocked), *flipped* (on its back: helpless, mash
dodge to right itself, takes ×2), *grabbed* (held; mash dodge to break free or autotomise), *blinded* (in ink: enemies
that hunt by sight lose you, and the screen around the crab darkens).

**Enemy state machine skeleton**: `Idle/Patrol → Notice → Approach → Tell → Attack (active) → Recover`, plus
`Stagger`, `Flee` and `Gone`. Each page lists its states, senses, timings and boxes. Enemies live in `Sim` as a `Vec`,
step in a fixed order, and use only the seeded generator, so replays stay exact.

**Drops** are real body parts or what the animal guarded, and feed the economy in `world.md`: food for the kitchen,
materials for the three claw branches (hammer, cut, rip) and armour.

## Concept sheets
Hand-written SVG per creature (`art/creatures/<name>/<name>.svg`, side view facing left, crab from
`art/crab/crab-parts.svg` at real scale), painted like this:
1. Render the art with ink strokes hidden (`resvg --stylesheet` with `[stroke="#2A140F"]{stroke:none}` and the labels
   hidden) at 2400 px, then `crabpaint oilphoto <in> <out> --attention off`.
2. Render the full art again and keep only the ink (a luminance-to-alpha filter), plus the labels alone.
3. Stack painted fills, ink, labels (one small wrapper SVG through `resvg`), then `oxipng -o 4`.
The alarm jelly's dark panels use the strokes themselves as the ink layer instead of the luminance filter.
