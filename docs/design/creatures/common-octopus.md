# Common octopus (*Octopus vulgaris*): boss

Concept sheet: `art/creatures/common-octopus/common-octopus.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** mantle to 25 cm, arms to 1 m, up to 9 kg (Wikipedia, citing Norman 2000 and National Geographic). Game:
  mantle 156 units, each arm 625. Arms are about four times the mantle: draw them that long.
- **Where:** rocky coastal floors, usually above 200 m (Wikipedia, citing Belcari et al. 2002). It lives in a den in a
  hole or crevice, often fortified with shells and stones; it leaves at dusk to hunt and returns at dawn [ADW].
- **Diet:** crabs, lobsters and bivalves are favourites (Wikipedia, citing Kanda et al. 2003). A crab is its classic meal.
- **Hunting:** pounces on prey that passes (Wikipedia); "webover": spreading the web over rocks and probing beneath with
  the arm tips (SeaLifeBase for *O. cyanea*, via search: not verified for *O. vulgaris*).
- **Venom:** salivary toxins (cephalotoxins) paralyse crabs: crabs injected with them showed hindered movement, then
  complete immobilisation and death; the toxin enters through a quick bite wound [GC21].
- **Camouflage:** changes colour, skin texture and posture to vanish on almost any surface; tells things apart by
  sight, touch and taste [ADW].
- **Ink:** a cloud about the animal's size that predators attack instead while it escapes (Wikipedia, "Cephalopod ink";
  a squid experiment found the decoy effect is mostly visual).

## Combat design
**Role: boss.** The crab's oldest enemy, in its den. Three phases that each use one real trick, and a reward that ties
into growing: its den.
- **Phase 1, Hidden:** it's part of the rock. Find the eye (a horizontal bar pupil). The lantern shows its outline when
  close. Walk past unaware and it pounces with no tell beyond a dark flush of the skin (colour flush before attacking:
  not verified).
- **Phase 2, Probe:** from its den it sends arm tips into the crevices where the crab hides. Each arm slides in slowly
  (the tell), then grabs on contact. Parry the tip (it recoils) or cut it with scissor claws; an arm hurt enough is
  pulled back for the rest of the fight.
- **Phase 3, Webover:** it rises, gathers its arms and spreads the web (the tell), then drops it over the crab like a
  net. Dodge out sideways. Caught = grabbed: break free or **autotomise a leg**; if held too long, a venomous bite.
- **Hurt:** at two-thirds and one-third health it **inks and jets** to another spot; the cloud blinds whoever is in it,
  including the crab.
- **Weaknesses:** the arms (each its own health), the eye; it can't webover in a low tunnel; ink is limited.
- **Defeat:** it jets away for good (animals want to eat, not die). The den is yours: a midden of shells, many of them
  crabs' (brutal), and a big empty sea-snail shell, the next size of shell-home (cozy).
- **Feel:** cozy: the trader in the whale-fall village is a different, deep-living octopus, so meeting this one
  colours that friendship; brutal: the sky closes over you; surreal: a rock that looks back.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Hidden | — | Camouflaged: invisible to the crab beyond the lantern radius except the eye. Notices by sight at 400 units, by touch at an arm's length |
| Ambush pounce | tell 30, active 10 | Only if the crab passes within 140 units unaware |
| Probe | tell 60 per arm, active until withdrawn | Up to 2 arms at once; tip grab hitbox circle r 14. Parry window 10 ticks at tip contact |
| Webover rise (tell) | 84 | Real timing not measured; tuned |
| Webover drop (active) | 10 | Hitbox: a 500-unit-wide arc under the web, down to the floor |
| Grabbed | ≤240 | Dodge presses −10 each; autotomy frees at once. Bite at tick 120 (venom), heavy bite at 200 (30 damage) |
| Recover | 70 | After a missed webover, arms spread on the floor: arm hits ×1.5 |
| Ink + jet | 30 | At 66% and 33% health: a cloud r 160 for 180 ticks (*blinded*), then reappears 600+ units away |
| Flee | — | At 0 health: jets off; den opens |

- **Boxes:** mantle/head hurtbox ellipse 156 × 110; eye hurtbox r 12 (×2); 8 arms as chains of 6 circles tapering
  r 14 → 3, each arm 40 health (a hurt arm withdraws). Web hitbox as above.
- **Numbers:** health 400 (mantle), poise 200. Pounce 20 + grab; venom bite 10 + *venom* (slow 50%, 2 damage/s for
  5 s, stacks). Block over the bite blocks the venom (the claw takes it).
- **Drops / reward:** the den midden: common shells, a large empty gastropod shell (shell-home material). No body
  parts: it gets away.
- **Scenario tests:** dodging sideways out of the drop takes nothing; caught crabs are grabbed; autotomy frees at
  once; the venom bite lands at tick 120 of a grab, not before; a parried probe tip recoils without grabbing; an arm
  at 0 health stops probing; ink at 66% and 33% exactly once each; inside the ink a sight-hunting enemy loses the crab.

## Built so far
**Placed** in its den at the end of the far rim (`Level::den`; x 8650, its back to the wall, facing the way in). To try
it headless: `cargo crabsim --script "start x=8200 y=-1900; @0 spawn=octopus@8650,-1900 walk=right"` (crabsim puts in only the
enemies a script spawns).

**Simulation** (`crates/sim/src/octopus.rs`, numbers in `tuning.rs` as `octopus`, tests in `crates/sim/tests/octopus.rs`;
also on a flat test floor with `cargo crabsim`, e.g. `--script "start x=200 y=0; @0 spawn=octopus@480,0
walk=right; @62 parry"`):
- **Hidden:** a rock; `hidden` until found out (its eye low on the rock). It watches a crab it sees within 400 and
  pounces on one whose middle comes within 140 of its front: `flush` (30 ticks, the tell), `pounce` (10; its body
  lands 140 out, circle r 40), 20 damage and held. Any blow on it gives it away (it fights from its den instead).
  Judgement call: "unaware" isn't a crab state; being close is enough, since the crab's claws can't reach it from
  140 out. The tell is the flush; the answers are dodge, parry (it staggers) or block.
- **Probe:** found out and the crab within an arm's length (625) of the arm roots, it stays in its den and sends arm
  tips, two out at most, one every 45 ticks at least, taking turns. A tip slides 60 ticks to where it just touches the
  crab's near side (so the touch lands on the 60th), then feels there 120 ticks and draws back. On a touch (tip circle
  r 14): parried, it recoils (snaps in, sore for 150); blocked, pushed back (each block drains 15 poise, so a guard
  held forever breaks); through, 5 damage and grabbed. Any blow on an arm takes its own health (40) and pulls it back;
  at 0 it is lost for the fight. Scissor claws' jab cuts an arm x2.
- **Webover:** with the crab's middle within 250 of its own (under its web) and 220 of clear water over the crab (no
  low roof of rock: it can't webover in a low tunnel), it `rise`s 84 ticks (the tell; its aim follows the crab until
  the last 24), then `drop`s the web over 10: a net 500 wide coming down from 150 to the floor. Whatever is under it
  when it gets down to it is caught (8 damage, then held); raised claws don't stop a net, a dodge's invulnerable
  moment does. Missed, it lies 70 ticks `spread`, arms on the floor and mantle low (eye in reach): an arm hit then
  counts x1.5. Then 120 ticks before the next webover (it probes meanwhile).
- **Grabbed** (after a pounce, a tip or the web): held up to 240 ticks, each dodge press 10 off (the moray's hold, its
  struggle now per creature). At 120 ticks held, the venom bite (10 + *venom*); at 200 the heavy bite (30).
- **Venom** (the crab's status, for any bite or sting): each dose 2 damage/s for 5 s, doses stack, and the crab walks
  at half speed while any is in it. A bite on raised claws carries none (the claw takes it); while held the claws
  can't be raised, so the octopus's bites always carry it.
- **Autotomy** (any hold): down + dodge, or dodge held for half a second (phones have no down), drops a walking leg:
  free at once, 8% slower walking per leg (at most 4). They grow back when the crab wakes after dying (its moult in
  the game, `world.md`) until moulting itself is built (#177), where they should regrow instead. The puppet doesn't
  show the missing leg yet.
- **Hits:** a claw finds the eye (r 12, x2) before the mantle (ellipse 156 x 110, 400 health, 200 poise) before an arm
  that is out. It keeps to its den (blows don't shove it). Left alone 2 s with nobody within 900, it mends (arms too)
  and is a rock again, back in its den.
- **Ink and jet:** the first time a blow takes its mantle to 66% or below, and again at 33%, it inks (a cloud r 160
  where it was, for 180 ticks) and jets for 30 ticks (`ink`; nothing reaches it), then reappears at least 600 away
  (away from the crab if there's room: flat floor, no more than 200 above or below, clear of rock; tried at 600 to
  1000 on each side, else it stays) and fights on from there. One blow past both shares inks once.
- **Blinded:** a crab whose middle is inside a cloud. Creatures that hunt by sight (`sight` in their tuning: the
  mantis, the octopus) lose it and can't notice it; smell (moray) and feeling steps (snapping shrimp) still work, and
  the octopus still feels a crab within its arms' reach. The crab's dump status says `blinded`. Not drawn yet (the
  screen darkening around the crab in the bestiary would be a full-frame overlay, which the art rules forbid: the
  cloud will be drawn as a shape instead).
- **Flee:** at 0 health it jets off for good (`flee`, 30 ticks) and is gone, leaving no body to eat.
- **Its den:** then open. Its midden of old shells is shell grit: 4 pieces per crab, as many at a time as cling to
  the shell, taken by walking onto the den (where the octopus first lived) and built into the home like any grit.
  Judgement call: the bestiary's "big empty sea-snail shell, the next size of shell-home" is shown as grit for growing
  the home you have (4 pieces is most of the way to its last stage); moving into a bigger shell is its own feature.

**Puppet** (`crates/crabrig/src/octopus.rs`, parts `art/creatures/common-octopus/common-octopus-parts.svg`, atlas in
`assets/creatures/`; drawn by `src/octopus.rs`): a kit like the moray's. The head (held by its arm crown) with the mantle
hinged at its back and the eye (horizontal bar pupil, eye-horn papilla); a pale copy of head and mantle faded in to
blanch; the web (stretched by the game); the camouflage skin (the whole animal flat on the rock in rock colours,
papillae up, its eye its own part so it glints in the dark); one set of eight tapering arm segments, suckers underneath,
which the game strings from the crown to each arm's tip in the simulation (the far four shaded darker, behind the body).
Poses from its state:
- **hidden:** the camouflage skin, arms tucked under; out of the lantern only the eye shows.
- **flush:** the skin shown, pressed low and darkening.
- **pounce:** arms thrown forward.
- **probe/den:** up on its arms, mantle breathing, arms drawn to their tips; a faint mark where a sliding tip will touch.
- **rise:** it climbs 45 on its arms, which spread in a ring, the web opening like an umbrella; a line on the floor
  shows the net's 500, firm once the aim locks.
- **drop:** the web a dome to the floor over that line.
- **spread:** arms flat on the floor, mantle low.
- **grab:** arms over the crab's back.
- **hurt:** blanches pale for a moment.
- **ink:** a dark cloud (r 160, in front of the crab, which is lost from view inside it), while it jets off pale, arms
  streaming.
- **gone:** not drawn.
Not drawn: the crab's missing legs after autotomy, and venom on the crab.

## Rig notes
Parts: mantle, head (with eye and an eye-horn papilla), siphon, web (one deformable part between the arm bases, or 8
wedge parts), 8 arms each as a chain of 6–8 tapering segments with the suckers painted on the underside (the inner
side), 4 near arms bright and 4 far arms darker behind. A camouflage skin variant (rock palette, papillae up).
Pivots: arm roots in a ring under the head; mantle hinges at the back of the head.
Key poses: **idle/hidden** (flattened on rock, arms curled under, only breathing pulses of the mantle);
**tell** (mantle up, arms lifted in a ring, web stretched like an umbrella, eyes up); **webover** (arms spread to the
floor, web a dome); **recover** (arms trailing back, mantle low); **hurt** (blanch pale, ink puff from the siphon,
arms streaming behind in a jet).

## Sources
- [GC21] Gonçalves & Costa (2021), "Cephalotoxins: a hotspot for marine bioprospecting?", *Front. Mar. Sci.* 8:647344. <https://www.frontiersin.org/journals/marine-science/articles/10.3389/fmars.2021.647344/full>
- [ADW] Animal Diversity Web, *Octopus vulgaris*. <https://animaldiversity.org/accounts/Octopus_vulgaris/>
- Wikipedia, "Common octopus" and "Cephalopod ink" (start points). <https://en.wikipedia.org/wiki/Common_octopus>
