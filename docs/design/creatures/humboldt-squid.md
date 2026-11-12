# Humboldt squid (*Dosidicus gigas*)

Shared rules: `README.md`. Code: `crates/sim/src/creatures/squid.rs`; tests `crates/sim/tests/humboldt-squid.rs`;
rig `crates/crabrig/src/bestiary/squid.rs`; puppet `src/creatures/squid.rs`.

## Biology that matters
- **Size:** common length 80 cm, maximum total length 400 cm [SLB]; one physiology paper gives up to 2.5 m and 50 kg
  (search snippet only: not verified). Game adult: **80 cm total length, fin to arm tips** (500 units; the
  common length, tentacles extra), mantle and head 74 cm; hurtbox 492 x 87 over the mantle and head. Size audit:
  it was drawn 102 cm (SeaLifeBase's 80 cm is a total length, not a mantle length).
- **Where:** pelagic, eastern Pacific from the Gulf of Alaska to Chile; 0-2000 m; rises to 0-200 m at night and goes
  down to 800-1,000 m or deeper by day [SLB]. Part-time resident of the oxygen minimum zone, with metabolic
  suppression and a high-affinity oxygen carrier (search snippet of a physiology paper: not verified). The game
  puts it over the canyon shelf, in the twilight band.
- **Diet:** copepods, amphipods, krill, shrimp, **red crabs (*Pleuroncodes planipes*)**, other squid, octopods and
  fish; cannibal only on juveniles [SLB]. Feeds most actively at dusk and dawn [SLB]. A swimming crab is on the
  menu in kind (red crabs); a 12 cm walking crab as prey is an extrapolation.
- **Skin:** chromatophores are reddish-brown only [search snippet, not verified]. The animal-borne video study
  describes **flashing** (a global pattern change) and **flickering**, mostly when other squid are in view, read as
  signalling; its displays work mainly in the time domain (rate of change), seen to 70 m without lights [RGB]
  (abstract via search result: the full paper was not opened). That *flashing and changes in flashing rate matter*
  is the real basis of the tell. Red and white as the two colours, and flashing at a hunted crab, are design
  readings: not verified.
- **Tentacles:** squid grab prey with two muscular feeding tentacles whose tips carry suckers or hooks, then drag it
  to the mouth [NG]. In a small squid (*Doryteuthis*) the stalks elongate (up to 61% of mantle length) so the clubs
  strike first, then shorten to pull the prey in [FRONT]. Humboldt-specific strike speed and reach: not verified.
  The game's 450-unit reach (72 cm) is about 0.7 mantle lengths; strike speed and the pull are tuned.
- **Beak and flight:** the beak is the hard part left over (the drop). Inking as an escape is a general squid
  behaviour; Humboldt-specific facts: not verified.

## Role and register
**Open-water hunter, twilight (hunted by sight).** Lesson: *read the flashing skin; the tentacles shoot far; cut a
held tentacle.* Unlike the others in the README table it has no body to dodge: the danger is a long thin line that
you see being aimed, the answer is leaving the line (dodge, block, parry) and, if caught, fighting the rope rather
than the animal.

## Combat design
Round: it hovers at the crab's depth with its arm crown 350-500 units off and circles (drifts up and down), waits
1.5-4 s, then:
1. **Tell, 384 ticks (3.2 s), two beats.** Beat one (312 ticks): the skin flips red and white, from every 48 ticks
   to every 8, the aim tracking the crab. Beat two (72 ticks, 0.6 s): the tentacles are drawn tight together,
   pointing along the line, **aim locked**. `Cue::Tell` is emitted automatically at the start.
2. **Strike, 6 active ticks.** The tentacles shoot along the locked line, tips at full reach (450) after 3 ticks; a
   thin capsule (club radius 22) hits a crab it touches along the line. Parryable (timing: block pressed in the 10
   ticks before the tips arrive), blockable, dodgeable (the dodge's invulnerable ticks).
3. **Hit = snare then grip** (max cost 4 + 4x3 = 16 shell, plus nothing else; the cap is 25):
   - *Snare:* the clubs stick; the crab is pulled in at 200 u/s and **can still fight**. The tentacle is a soft
     target: hits on the squid are taken by the tentacle (its own 20 health, the squid's shell is untouched),
     **cutter x2, crusher x0.5**; cut through, it lets go and **staggers** (100 ticks). A crab that out-walks the
     pull and stretches it past 500 units slips free; the clubs also let go after 5 s.
   - *Grip:* inside 170 units the arms close: the shared hold (no acting; dodge presses take 24 ticks off a 120-tick
     hold; autotomy frees too), 3 damage every 30 ticks. Everyone else (a co-op friend) can still hit the squid.
4. **Recover, 144 ticks:** it jets back for the first 48; the mantle and head are open (head hits x1.5 while
   recovering).
- **Block:** holds the tentacles off: no snare, no shell damage, 30 poise; the squid recovers 72 ticks.
- **Parry:** no damage; it staggers 100 ticks.
- **Escape:** walking out of the 450-unit reach during the tell makes it miss; leaving 1,000 units from its home
  makes it give up. It **breaks off** the tell (48-tick recover, hunt dropped) if the crab reaches a Refuge, Mouth
  or Tunnel landmark or holds kelp or coral, and will not tell at a sheltered crab at all. It never enters tunnels.
- **Driven off:** health 140, poise 90 (smash 25 poise: four smashes stagger it for 90 ticks). Below 40% it **inks**
  (a cloud, 140 radius, 6 s) and jets away for 2 s at 520 u/s, then is out of the world for 60 s (no body to eat),
  and returns whole. Killed outright it leaves its body (food) and a beak; respawn 8 minutes.
- **Feel:** cozy: none; brutal: a patient, flickering animal that decides where you are; surreal: a red-and-white
  strobing shape in the dark water with a line that arrives before the sound does.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Idle | | drifts home at 60 u/s; sees a crab within 1,100 sideways and 200 up or down |
| Notice | 60 | faces the crab; skin flickers |
| Approach ("circle") | | to the ring (crown 350-500 off, same level +/-200); 170 u/s; waits 90-240 ticks, then tells if the line is clear and the crab is not sheltered |
| Tell | 384 | flash 312 (period 48 to 8), lock 72; checks shelter every 24 ticks |
| Attack | 6 | tips out in 3 ticks; hit test along the capsule from the crown; terrain blocks |
| Grab | up to 300 + 120 | snare (pull), then grip |
| Recover | 144 | jet back the first 48 |
| Stagger | 100 (sever, parry) / 90 (poise) | |
| Flee | 240 jet, then away 7,200 | ink, then out of the world |

- **Boxes:** body 625 x 110; while snared the hurtbox grows out along the tentacle (`reach`) so a claw can reach it;
  the arm crown (tentacle origin) is at the front of the body, 55 up.
- **Numbers (real vs tuned):** size from [SLB]/design; everything else tuned. Health 140, poise 90 (refill 20/s after
  1.5 s), strike 4 damage / 30 poise, grip 3 per 30 ticks, tentacle 20 health.
- **Statuses used:** `grabbed` (the grip), blocked-by-nothing; none new. The snare's pull sets the crab's velocity
  along the rope each tick (`!remote` crabs only).
- **Drops (`DROPS`):** `squid beak`, and its body as food. It eats fish, squid and crustaceans, including red crabs.
- **Placement:** over "canyon shelf crossing", at the shelf's top +/-: home found by name, 450 above the shelf's
  floor edge, in the x range that puts the nearest refuge (the left wall's coral stand) 400-600 units away; it roams
  +/-600 and 220 up or down, clear of the sixgill's "canyon deep water". Never a pit landmark. Chambers and pockets
  of the world build are far away.
- **How it differs from the rest:** the mantis (club, ground), the triggerfish (water jet, mouth) and the moray
  (lunge from a crevice) all hit at close range; the squid hits from 450 across open water, after a long readable
  tell, and its answer when it lands is a fight against a rope, not a mash.

## Rig notes
Parts: body (mantle, root), fin (flaps), head, eye, arms-upper/arms-lower (crown, breathing), tentacle-a/b
(stretched and turned to the sim's aim) and club-a/b (at the stalk tips), body-flash and head-flash (pale overlays
shown and hidden at the flash rate). Poses read only from the sim: idle/move (fin ripples), notice (flicker),
tell flash (tentacles open and waving, strobing), tell lock (tight, along the aim, body tilts), strike (full
reach), snare/grip (tentacles to the crab), recover, hurt/stagger (wobble), flee (ink cloud; hidden while away),
defeat (belly up). No bubbles, no filters.

## Scenario tests (`crates/sim/tests/humboldt-squid.rs`)
Placement; tell before blow, 384 ticks, `Cue::Tell`; flash period shrinks and aim tracks then locks; unanswered
strike -> snare, pull, grip, cost <= 25; cutter sever frees and staggers (squid shell untouched); cutter beats
crusher; parry, block, dodge; out-walking; leash; sheltered crab never told at; break-off; round cost <= 25; poise
stagger, ink and flight and return; tunnels; determinism; co-op (two sims); `feels` is the only reaction.

## Sources
- [SLB] SeaLifeBase, *Dosidicus gigas*. <https://www.sealifebase.se/summary/Dosidicus-gigas.html> (opened)
- [NG] National Geographic, deep-sea squid prey capture (2013). <https://www.nationalgeographic.com/news/2013/8/130827-deep-sea-squid-prey-capture-ocean-animals-science/> (opened)
- [FRONT] Frontiers in Physiology 2019, tentacular strike in *Doryteuthis opalescens* (another species). <https://www.frontiersin.org/journals/physiology/articles/10.3389/fphys.2019.01558/text> (opened)
- [RGB] Rosen, Gilly, Bell, Abernathy, Marshall 2015, "Chromogenic behaviors of the Humboldt squid", J Exp Biol 218:265. doi:10.1242/jeb.114157 (abstract seen through a search result only)
