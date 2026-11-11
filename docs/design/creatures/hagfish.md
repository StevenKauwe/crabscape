# Hagfish knot (*Eptatretus stoutii*, issue #583)

Micro-zone crowd at the whale fall. **The one thing it teaches: don't hit what slimes. The slime clogs your gills; take the
prize around them.** One enemy in the sim, a knot of 4 to 6 fish.

## Biology that matters
| Fact | Source |
|---|---|
| Pacific hagfish are 30 to 63.5 cm long (game: 50 cm each = 312 units; the knot's box is 150 x 56 because they are coiled) | [ADW] |
| Scavengers: dead or dying fish, mammals, carrion; they burrow into carcasses | [ADW] |
| They tie a knot in the body and slide it along: grips and tears food, buries into carcasses, escapes predators, wipes off slime | [ADW] |
| Slime is released almost instantly and swells into a sticky gel on contact with seawater; it can suffocate an attacker | [ADW] |
| Slime fills a predator's mouth and gill chamber in about 0.4 s and the predator lets go, gagging; it clogged gills of sharks, conger eels and others; the hagfish stayed unhurt, often kept feeding. Slime is released only from the glands the predator touches | [ZIN11] |
| Slime also deters other scavengers from the food | [ZIN11] |
| Hagfish are listed among whale-fall scavengers | not verified (the MBARI whale-fall page could not be opened) |
| Crabs breathe with gills, so slime clogging a crab's gills is the same mechanism; its strength on a crab is a design call | design |

## Role and register
Micro-zone crowd, midnight floor. It is never a hunter: it feeds and ignores a careful crab. It is a toll on the greedy:
the prize (a whale bone on the carcass) lies behind the knot, and the way through is to not hit it, not touch it, hop or
swim over it, or pay in reserve. Different from the README's other creatures: no approach, no chase, no one big blow; the
threat is an area that wears the crab's breath down.

## Combat design
- **Knot (feeding):** stands still on the carcass. Nothing happens unless a crab **strikes** it (its health drops) or **touches** it (the crab's body box overlaps the knot box).
- **Slime tell (0.4 s = 48 ticks):** the knot writhes faster and the pores along the fish flush pale. Then the **cloud**: radius 140 at six fish (smaller with fewer), 6 s (720 ticks), centred on the knot.
- **Cloud effect (only a crab on the device running the knot, only a crab not tucked in):** per second at six fish, reserve -0.12, shell -1 (never below 1), poise -8 (never below 25). Block and parry do nothing against it. Leaving the cloud ends it at once; a dodge or a walk of about 100 units out of the knot's edge is enough. A fresh strike inside the cloud thickens it again (back to 6 s, no new tell).
- **Rasp (the only blow):** a crab that stays overlapping the knot for 150 ticks (1.25 s) inside a cloud is rasped. Tell 0.8 s (96 ticks, writhing toward it, the aim locks 24 ticks before the blow), active 8, damage 5, poise 10, parryable and blockable, then recover 150. A parry staggers the knot 60 ticks. It drops the tell if the crab leaves the cloud, and never rasps a crab at 30 shell or less.
- **Touch rest:** after a cloud ends, a mere touch cannot slime again for 10 s (a strike always can). No standing in the knot forever.
- **Answers:** do not strike; hop/swim over it to the bone; strike and then leave the cloud at once; parry the rasp; dodge out. Bringing the knot down costs about 5 smashes while standing in the cloud: about a third of a full reserve and 6 shell.
- **What breaks:** health 60 for the whole knot; each 20 lost, one hagfish leaves (6 fish above 40 health, 5 at 40 or less, 4 at 20 or less) so the cloud shrinks and drains less (scaled by fish/6). It has no poise (1,000,000): a soft rubbery knot does not stagger; only a parry throws it back. Unhit for 10 s it mends 1 health per second.
- At 0 the knot **disperses** (`State::Gone`): the cloud ends, a body lies there to eat, `DROPS = ["slime thread"]`, and another knot settles after 5 minutes.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle ("feed", "slimed" while a cloud hangs) | - | counts overlap; reacts to strike/touch |
| Tell ("slime-tell") | 48 | target = nearest crab; Cue::Tell emitted by the sim |
| Tell ("rasp-tell") | 96 | aim follows target until 24 ticks before the end |
| Attack ("rasp") | 8 | head circle (r 36) travels from the knot's middle to the aim by 50 + half body |
| Recover | 150 | |
| Stagger | 60 | after a parry only |

Numbers: real = fish length, slime speed (~0.4 s), knotting; tuned = everything else. Its `State` (in `Extra`): `cloud_left`, `tell`, `seen_health`, `touch`, `owed` (a blow that landed while it could not answer), `rest`. Slime tuning consts are at the top of `crates/sim/src/creatures/hagfish.rs`.
Co-op: the knot's step runs on one device; a crab with `remote` is never changed (no drain, no rasp). The runner follows `e.target`, which is the crab that struck or touched it, so its device applies the slime to its own crab. A bystander crab whose device does not run the knot is not drained (accepted). The rasp outcome goes through `feels` like any blow.

## Where it lives, and the encounter
The carcass centre is on the lower canyon floor `CARCASS_EAST = 600` units east of the "narrows lock" landmark's right edge (x = 18,680 today), exposed as `hagfish::carcass(level)`; the knot sits on it. The whale bone (the prize; the skills/world builds own the resource node) is `hagfish::prize(level)`: 120 units east of the carcass centre, behind the knot as the crab arrives from the west. The isopod sits about 500 units away, amphipods roam here. The knot is 240 units from the coral stand (a refuge) next to the lock and 190 from the chimney mouth; the 400-unit rule cannot hold at the shared carcass position, and the knot never leaves home, so it does not matter (`no_hunter_roams_inside_a_tunnel` passes: give-up range 150, no approach).

## Rig notes
Six fish as separate parts (`fishN`, `headN` with `eyeN` and `barbelN`, `poresN`) over a `body` tangle, facing right, true size. The game hides fish that have left (highest number first), writhes the fish, flushes `poresN` (scale 0 -> 1) in the slime tell and while the cloud hangs, and stretches `fish0` for the rasp. The cloud is its own translucent blots (not a filter). The whale carcass (backbone, four ribs per side, a skull section, the prize bone) is drawn in `src/creatures/hagfish.rs` from plain sprites at real scale (no sprite sheet).

## Scenario tests (`crates/sim/tests/hagfish.rs`)
Placement at the carcass; no slime unless struck or touched; strike -> tell -> cloud and a Tell cue; slime drains reserve, chips shell but never kills, ends; leaving the cloud ends the drain; staying in the knot gets rasped after a 96-tick tell, one blow at most 5; parry in the window; beaten (fish leave, then gone, never staggers); remote crab untouched and a parried outcome fed through `feels` reaches the same state; determinism.

## Sources
- [ADW] Animal Diversity Web, *Eptatretus stoutii*. <https://animaldiversity.org/accounts/Eptatretus_stoutii/> (opened)
- [ZIN11] Zintzen, Roberts, Anderson, Stewart, Struthers & Harvey (2011), "Hagfish predatory behaviour and slime defence mechanism", *Scientific Reports* 1:131 (read on PubMed Central, <https://pmc.ncbi.nlm.nih.gov/articles/PMC3216612/>)
