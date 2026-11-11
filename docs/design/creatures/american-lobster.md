# American lobster (*Homarus americanus*)

Rig parts: `art/creatures/american-lobster/american-lobster-parts.svg`. Shared rules: `README.md`.
Code: `crates/sim/src/creatures/lobster.rs` (numbers are named consts at its top), tests `crates/sim/tests/lobster.rs`.

## Biology that matters
- **Size:** adults commonly 20-61 cm, longest recorded body 64 cm excluding claws [WP]; ADW gives about 25 cm as the
  commonly caught size [ADW]. Game adult: **30 cm body = 190 units**, tail fan to the front of the carapace, claws and feelers extra (size
  audit: the body was drawn 39 cm).
- **Where:** cold rocky bottoms (also sand and mud); usually 4-50 m, found as deep as 480 m [WP]; ADW: up to 365 m
  [ADW]. It spends most of the day in a burrow or among rocks and comes out for food; when a predator approaches it
  retreats into the burrow [ADW]. Fits the canyon's cold floor.
- **Diet:** clams, crabs, snails, small fish, algae, eelgrass; mostly at night; usually a scavenger but captures its own
  prey [ADW]. Molluscs, echinoderms (sea urchins), polychaetes and other crustaceans dominate [WP]. A 12 cm crab is in
  its diet class (crabs: [ADW]).
- **Two different claws:** the larger claw has thick teeth for crushing, the smaller sharp teeth for cutting [ADW]; the
  big one is the "crusher" with rounded nodules, the small one the "cutter" or "gripper" [WP]. Same plan as our crab.
- **Fighting:** adults generally fight when threatened unless they have lost their claws; they defend what they
  claim and signal rivals with urine plumes [WP]. Solitary, with social hierarchies; higher-status males secure
  shelter [ADW].
- **Tail flip:** the abdomen flexes and the animal shoots backward: the escape response [WP: post-larval lobsters rely
  mainly on it when threatened].
- **Not verified (no source opened):** the *meral spread* (claws raised and spread) as the opening display of a
  dominance fight, and claw locking. Atema & Voigt and Karavanich & Atema are the papers to open; the pubmed search
  returned nothing usable in this pass. The guard pose is built on this behaviour and is therefore flagged.
- **Not verified:** that a lobster parries a blow with a claw snap; the counter beat is a design reading of claw
  defence.

## Role and register
**Chamber guardian and rival crustacean.** Teaches **the mirror duel**: it blocks and parries like you; feint and break
its guard. Unlike the grouper (suction), triggerfish (jet) and octopus (ambush) it has no trick: it has your own kit.

## Combat design
- **The guard:** noticing a crab (sight, 400 u) it raises and spreads its claws (36 ticks), walks out and stands
  guarding at about 125 u from the crab. In guard every blow does **x0.2** damage and its poise damage drains a
  separate **guard poise** (45) instead of its own poise; at 0 the guard breaks (72 ticks staggered, open).
- **The counter beat:** if the crab *begins a crusher smash* in front of its guard, its claws snap shut for **48 ticks**
  (readable). A smash that lands in the beat is **parried**: no damage to it, the crab takes 50 poise and a push, no
  health. After the beat the claws reopen for **40 ticks, unguarded** (full damage, normal poise).
- **Its attacks (each preceded by a Tell, which plays the swell cue):**
  - *Cutter snip:* tell 60 ticks (cutter opens), active 4, damage 10, poise 14, parryable, recover 36.
  - *Crusher crush:* tell 120 ticks (crusher raised and opened wide), active 6, damage 22, poise 40, parryable;
    blocking it cracks the crab's guard 180 ticks (`armour_break`), recover 66.
- **The crab's answers:** *parry* either blow (45-tick counter window; x2 smash on its open claws); *dodge* through;
  *block* the snip (the crush costs the guard); step back out of its planted tell; **jab** the guard (a jab never
  starts the counter beat and drains 8 guard poise); **feint** a smash to burn its counter beat, then smash while it
  reopens; hit it in any recovery, stagger or reopen.
- **It flees:** hurt to 35% it does the **tail flip**: shoots backward home (800 u/s for 45 ticks, then scuttles),
  spent 100 ticks (unguarded: the punish), then holds its den: guard up, snips what comes within 300 u, never
  crushes, never leaves. Unhit 15 s with nobody near it mends. Beaten to 0 it leaves its body (food) and a crusher
  fragment.
- **Gives up:** never follows more than 500 u from its den; loses a crab beyond that, in a refuge/mouth/tunnel or
  holding kelp or coral, and walks home calm.
- **Feel:** cozy: a shy animal behind its midden; brutal: a rival with your own claws; surreal: a crab-shaped fight
  against an animal that is not a crab.

## Implementation spec
| State (generic) | Sub | Ticks | Notes |
|---|---|---|---|
| Idle | Rest | - | Walks home at half speed; notices at 400 u (300 if denned) |
| Notice | Spread | 36 | Claws rise; unguarded |
| Approach / Notice | Guard | - / 50-90 pause | Walks 130 u/s (crab 270) to 125 u; guarded |
| Notice | Counter | 48 | Claws shut; a landing smash is parried |
| Notice | Reopen | 40 | Unguarded |
| Tell / Attack / Recover | Snip | 60 / 4 / 36 | Planted; no turning |
| Tell / Attack / Recover | Crush | 120 / 6 / 66 | Planted |
| Stagger | Parried / Broken / Dazed | 60 / 72 / 60 | Parried by the crab; guard broken; poise broken |
| Flee | Flip | until home | 800 u/s for 45 ticks, then 260 u/s, facing kept |
| Recover | Spent | 100 | Unguarded |
| Idle | (denned) | - | Guard up, snips only |

- **Boxes:** body hurtbox 190 x 50; raised claws extend it forward by `reach` (60) so the crab's blows land on the
  claws; the snip circle is 70 u ahead of the head (r 26), the crush circle 80 u (r 32).
- **Numbers:** health 120, poise 90, guard poise 45 (refills 18/s after 100 ticks unhit). Real: two unlike claws,
  tail flip, den, sight. Tuned: every number above.
- **How it fits the shared skeleton:** `Enemy::takes` runs before its step and cannot be hooked, so at the start of
  its step it reads the health and poise it lost since its last step and, if it was guarding, gives back the guard's
  share (80%; 100% in a parry), undoes a poise stagger, drains guard poise, and parries. This also works for hits
  arriving from a co-op guest. A shared hook would make this direct: `takes` consulting the creature module.
- **Statuses:** none of its own; it shoves (poise, push) and cracks the crab's guard.
- **Drops (`DROPS`):** `lobster crusher fragment`; the body is food.
- **Where:** one den on the canyon floor in the still water by the wall, found by the landmark `canyon deep water`
  and the live floor, 400+ u from every refuge and clear of every tunnel, mouth, chamber and frame, off the tide
  column where it can. (The far rim, the brief's first choice, has no spot 400 u from both the far-rim refuge and
  the octopus den; it also drops 2,000 u in the world build.) The den's midden of shells is not yet drawn.
- **Encounter:** open floor, leash 500, a coral refuge is not within 400. The crab can leave by walking away. Differs
  from the others: it is the only guarded enemy; the answer is timing against a reaction, not an area.

## Rig notes
Parts (side view, facing right, feet on y = 0): carapace (root) with eye and belly shading; abdomen-a, abdomen-b and
tail fan (curl under for the flip); two antennae (sway); four walking legs (alternate when walking); crusher arm,
palm (with teeth) and finger; cutter arm, palm and finger. Poses (from `Enemy::phase`): rest (claws low), spread
(rising), guard/den (raised, fingers open), counter (shut in front), reopen, snip-tell (cutter finger opens),
snip (snapped), crush-tell (crusher reared wide), crush (slams down), recover, spent (tail drooped), flip (tail
curled), stagger/parried/broken (wobble), gone (limp).

## Scenario tests (`crates/sim/tests/lobster.rs`)
Tell lengths and cue; no blow or pair kills; parry staggers it; block cracks the guard on a crush; dodge takes
nothing; guard x0.2 and guard drain; jab not parried, smash parried (shove, no health, no loss); open guard full
damage; guard break and poise break; a scripted crab drives it off; tail flip then den (snips only); leash and
escape; determinism; remote crab untouched; placement; a two-sim co-op check (parry seen by both, no duplicate damage).

## Sources
- [ADW] Animal Diversity Web, *Homarus americanus*. <https://animaldiversity.org/accounts/Homarus_americanus/>
- [WP] Wikipedia, "American lobster" (start point; read only the first 100,000 characters). <https://en.wikipedia.org/wiki/American_lobster>
