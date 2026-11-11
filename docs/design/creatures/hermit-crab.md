# Hermit crab with stinging anemones (*Dardanus*, carrying *Calliactis parasitica*)

Rig: `art/creatures/hermit-crab/hermit-crab-parts.svg` (ink only). Shared rules: `README.md`. Issue #580.

## Biology that matters
- **The pair:** *Calliactis parasitica* is usually found on a gastropod shell carrying a hermit crab, often several per
  shell; the crab's partner stings predators (it releases thread-like acontia when disturbed) and eats scraps the crab
  knocks loose. The anemone starts the association by climbing onto the shell [W-CP]. *Dardanus calidus* is recorded as
  symbiotic with it, 3 to 30 m, in semidark caves and seagrass beds [SLB-DC].
- **Octopus deterrence:** octopuses avoid shells carrying *C. parasitica* but still attack shells of other hermits
  without it; in aquarium studies the pairing with *Dardanus arrosor* broke down, and octopus chemical signals
  prevented or reversed that [W-CP]. The Ross / Brooks papers named in the brief were not opened: not verified as
  sources, only the Wikipedia summary of the result.
- **Size:** the anemone reaches about 100 mm tall [W-CP]. Hermit size is not given by SeaLifeBase for *D. calidus*
  (blank). Game shell: 11 cm = 70 units (brief; not verified).
- **Withdrawing:** a hermit can retract fully into its shell and block the entrance with its claws [W-HC].
- **Shell fights:** crabs fight (rarely, when sizes differ a lot) for better shells; the initiator "raps" its shell
  against the other's in rhythmic strikes before pulling it out [W-HC]. The page credits Dowds & Elwood (1983); the
  Elwood & Briffa work in the brief was not opened.
- **Diet:** omnivorous scavengers, mostly nocturnal [W-HC]. Here a rival for the same carrion, not a hunter.
- Big-left-claw plug, "rap" timings and sting numbers are design, not measured.

## Role and register
**Micro-zone rival, the shell turtle.** Lesson: never hit the stinging shell; hit the claws when it reaches out.
A slow animal on its rubble patch that wants the same food as the crab. Cozy: a small neighbour with a garden on its
back; brutal: a hit shell stings back; surreal: the garden fights.

## Combat design
- **Pinch:** the big claw rears and opens (tell 72 ticks, 0.6 s), snaps (4 active), 9 damage, 15 poise, parryable,
  blockable. **Rap:** it rocks the shell back (tell 108, 0.9 s) and drives it forward ~36 units (8 active), 6 damage,
  30 poise, shove 140, parryable, blockable; an unblocked rap also envenoms (existing venom status: the anemones).
  Parry staggers it (100 ticks). Dodge through either.
- **Withdraw:** when a crab winds up a blow within 130 units it may shut in (70% chance, once per swing; lag 8 ticks,
  so a jab, wind-up 6, lands first but a smash, wind-up 14, finds the shell). Shut in for 180 ticks (`withdrawn`);
  then 240 ticks before it may again. Only from idle / notice / approach: never mid tell, attack, recover or stagger.
- **Shell:** every blow does 0.1 of its damage and cannot break poise or kill from full; the anemones **counter-sting**
  the crab that struck (3 damage, 8 poise, venom if unblocked; blocked = no venom; at most once per 60 ticks; not
  parryable). The sting is a passive contact like the alarm jelly's: the warning is the withdrawn shell itself.
- **Out and committed** (tell, recover, stagger) the soft claws and legs take 1.5x. Recover is 84 / 100 ticks and is
  the punish window; it cannot withdraw out of it.
- **Hurt to 30% (15):** when it next can act it shuts in and **sulks** until no crab is within 520 units for 2 s, then
  comes out mended (full). It can only be finished by burst damage or in a stagger window. If it reaches 0 while out:
  drops.
- **Escape:** it never chases past 450 units from home, ignores a crab in a refuge or on kelp/coral, walks 55 u/s.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle | - | amble within 110 of home at 22 u/s, rests; notices a crab within 250 |
| Notice (`claw-up`) | 48 | turns, big claw raised |
| Approach | - | 55 u/s until the crab is within 80 of its head and level |
| Tell | 72 pinch / 108 rap | committed; `Cue::Tell` emitted |
| Attack | 4 / 8 | circle at claw / shell front |
| Recover | 84 / 100 | open, soft parts x1.5 |
| Stagger | 90 (poise) / 100 (parried) | |
| Withdrawn | 180 | shell x0.1, sting |
| Sulk | until crab gone 240 | |
- Body hurtbox 70 x 55 (the shell). Health 50, poise 40 (refill 20/s after 120 ticks). Respawn 4 min.
- Implementation note: `Enemy::takes` knows nothing of a shell, so shut in the hermit parks real health in
  `State::banked` and a 10,000 shield in `vitals`; each tick it reads what a blow took from the shield. Hits during the
  hit-stop are corrected one step late. The x1.5 soft-part bonus is applied the same way (an extra half after the
  hit). A shared `takes` hook would make both exact.
- **Co-op:** blows reach only `!remote` crabs; `feels` is the only reaction to an outcome and ignores sting outcomes.
  The sting goes to every local crab in reach with a swing out (the attacker's own device judges its own crab).

## Drops and food
`DROPS`: "whelk shell" (shell-home material: the prize), "anemone"; the body is food.

## Where it lives
One in the arrival basin (`basin` landmark, 90% across, x about 3250), rubble floor, 400+ from the basin's rest
crevice. Nearest refuge (kelp stand at the lip) is 190 units: the basin is only 500 wide, so the 400 refuge rule
cannot be met there or in the far basin (refuges 120 away; the mantis mouth sits mid-basin). It never walks over a
pocket mouth or rock (`footing`). Differs from the others: a slow, fair, readable first rival that teaches
not-hitting-armour before the mantis; no open-water pursuit.

## Rig notes
Parts: shell (whorl lines), body, head (two eyestalks), small claw, two legs, big claw (on the shell so it can plug the
aperture) with hinged finger, three anemones with tentacles. Poses: idle (ambling legs, sway), claw-up, pinch-tell
(claw rises, finger opens), pinch, rap-tell (shell rocks back), rap (shell drives forward), recover, stagger
(wobble), withdrawn/sulk (body hidden, claw plugs, anemones stretched), flare on sting, gone (lying).

## Scenario tests (`crates/sim/tests/hermit_crab.rs`)
Placement; tell length and cue before each blow; parry; pinch 9 / rap 6 + venom / blocked rap; x1.5 when out; shell
x0.1, no stagger, sting; blocked sting no venom; jab lands before shutting in, smash gets stung sometimes; poise
break; sulk then mend; finish while staggered low; scripted beat/drive-off; escape; no cycle over 25 damage;
determinism; co-op two-sim.

## Sources
- [W-CP] <https://en.wikipedia.org/wiki/Calliactis_parasitica> (opened)
- [W-HC] <https://en.wikipedia.org/wiki/Hermit_crab> (opened)
- [SLB-DC] <https://www.sealifebase.se/summary/Dardanus-calidus.html> (opened)
