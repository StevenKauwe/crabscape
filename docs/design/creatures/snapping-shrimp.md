# Big-clawed snapping shrimp (*Alpheus heterochaelis*)

Concept sheet: `art/creatures/snapping-shrimp/snapping-shrimp.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** up to 5.5 cm body length [HS98]; snapping shrimp generally 3–5 cm [ALPH]. Half our crab's shell: 34 units.
- **Where:** littoral oyster banks, alone or in pairs (Gulf of Mexico population) [HS98]; snapping shrimp live on
  reefs, seagrass and oyster reefs and most dig burrows they hunt from [ALPH]. Shallows rubble.
- **The snap:** one claw is a huge snapper. The dactyl is cocked open (~100°) by opener and closer muscles working
  against each other, then a second closer muscle fires it shut [HS98]. Closing makes a jet fast enough to cavitate;
  **the bang comes from the bubble collapsing, not the claw** [VER00]. Jet ~25 m/s, peak pressure 80 kPa at 4 cm,
  218 dB re 1 µPa, under 1 ms, enough to stun or kill small prey [ALPH, citing VER00].
- **The real tell:** in fights the claw is held cocked for **about 500 ms** before the snap; most snaps are aimed at
  the opponent's claw from **~0.9 cm** away [HS98]. They sense jets with mechanosensory hairs on the claw [HS98].
- **Fights are endurance contests:** shrimp switch to cumulative assessment once they start snapping (Dinh & Patek
  2023, abstract via search: not verified).
- Not a crab predator: it defends its burrow. Partnerships with sentinel gobies exist in other alpheids (via
  Wikipedia, citing Karplus 1987: not verified) and are left out.

## Combat design
**Role: turret.** A small, fragile guard on a burrow (or a field of them) that controls space with a shock you can
read and avoid. Teaches spacing and jumping.
- **Attack:** *snap*: a short cone of shock in front of the claw. Stuns on poise break.
- **Tell:** the snapper claw swings open and holds. Open claw = step out of line.
- **The crab's answers:** *jump over* (the cone is low) or *dodge* through it (i-frames); *block* stops the damage but
  not the poise hit; **no parry** (you can't parry water). Close in during the re-cock and crush it.
- **Weaknesses:** tiny and soft; a long re-cock; tires after repeated snaps (design, from the endurance finding).
- **Feel:** cozy: busy little neighbours in the rubble, clicking all night; brutal: a field of them is a minefield;
  surreal: a thumb-sized animal that makes one of the loudest sounds in the sea.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| In burrow | — | Notices vibration: a crab walking within 120 units (sneaking at half speed doesn't count; tuned) |
| Guard | — | At the burrow mouth, feelers out, faces the crab |
| Tell: claw cocked | 54 | Real ~60 (500 ms), tuned slightly shorter |
| Snap (active) | 2 | Real < 1 ms |
| Re-cock | 72 | Tuned; the punish window |
| Tired | 180 | After 5 snaps in 10 s (design) |
| Flee | — | Into the burrow when hit; out again after 6 s |

- **Boxes:** body hurtbox 34 × 12. Shock: a cone from the claw tip, **38 units long** (6 cm: tuned up from the real
  centimetre or so so it reads in play), half-angle 25°, hugging the floor (height 30), so a jump clears it.
- **Numbers:** health 12, poise 10. Snap 6 damage, 45 poise; a poise break means *stunned* 30 ticks. Block: damage
  ×0.2, poise in full.
- **Drops:** food (a small, tasty shrimp). Its burrow can be a shortcut or a stash spot once it's gone.
- **Scenario tests:** a jump over the cone takes no hit; a dodge through it takes no hit; standing in it takes 6
  and 45 poise; a blocked snap takes ~1 damage and the full poise; parry has no effect; a crab sneaking at half speed
  is not noticed; it tires after 5 snaps.
- **In the sim:** `Kind::Snapper` (`crates/sim/src/enemy.rs`, numbers in `tuning.rs`, tests in `tests/creatures.rs`):
  feels steps (a crab moving faster than half walking speed within 120), guards its burrow, a 54-tick tell ending
  30 ticks cocked, a 2-tick snap whose shock is a box 38 long and 30 tall along the floor (unparryable), 72 ticks
  re-cocking, into its burrow when hit (once a fight, 3 s). Not yet: tiring after 5 snaps, the stun on a poise break.

## Rig notes
Parts: carapace with eye hood (eye shows through as a dark spot), 6 abdominal plates (the 2nd big and overlapping its
neighbours, the caridean hallmark), tail fan (telson + 2 uropods), antennule pair (2 flagella), antenna (one long
whip) + scale, snapper claw = arm (merus + carpus), palm (propodus) and dactyl (pivot at the palm top), small claw,
slender 2nd leg (jointed carpus), 3 walking legs near side + 2 far.
Key poses: **idle** (feelers sweep, legs shuffle); **tell** (dactyl swung up ~100° and held, body braced back);
**snap** (dactyl shut; a white bubble and ring at the claw tip for 6 frames, then a ripple); **recock** (claw lowered,
dactyl creeping open); **hurt** (tail flick backwards toward the burrow).

**Puppet (in the game):** drawn by `crates/crabrig/src/shrimp.rs` from the concept sheet (side view, facing right)
into `art/creatures/snapping-shrimp/snapping-shrimp-parts.svg`, exported to `assets/creatures/` (`cargo run -p crabrig
--profile art -- parts --creature snapping-shrimp`, then `-- export --creature snapping-shrimp`). Nine parts: carapace
(with the near legs), abdomen and tail fan, snapper claw and its finger, two feelers, two far legs, small claw. The
game swings the finger (tell, snap, re-cock), flicks the tail when hit or fleeing, and sweeps the feelers
(`src/critters.rs`). The finger is drawn a little heavier than life so the cocked claw reads at phone size. Ink style
only for now (no oil-painted copy yet).

## Sources
- [HS98] Herberholz & Schmitz (1998), "Role of mechanosensory stimuli in intraspecific agonistic encounters of the snapping shrimp (*Alpheus heterochaelis*)", *Biol. Bull.* 195:156–167. <https://archive.org/download/biostor-10891/biostor-10891.pdf>
- [VER00] Versluis, Schmitz, von der Heydt & Lohse (2000), "How snapping shrimp snap: through cavitating bubbles", *Science* (not opened; seen in search results, figures via [ALPH]). <https://portal.fis.tum.de/en/publications/how-snapping-shrimp-snap-through-cavitating-bubbles/>
- [ALPH] Wikipedia, "Alpheidae" (start point). <https://en.wikipedia.org/wiki/Alpheidae>
