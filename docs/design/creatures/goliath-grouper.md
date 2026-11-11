# Goliath grouper (*Epinephelus itajara*)

Shared rules: `README.md`. Code: `crates/sim/src/creatures/grouper.rs`, tests `crates/sim/tests/goliath_grouper.rs`,
rig `crates/crabrig/src/bestiary/grouper.rs`, puppet `src/creatures/grouper.rs`. Issue #578.

## Biology that matters
- **Size:** up to 2.5 m and 363 kg [FM]; FishBase gives 250 cm TL max, common 150 cm, max weight 455 kg [FB]. Game
  adult: 2 m = 1,250 units long, 420 units tall with fins (true size, never scaled for drama).
- **Where:** shallow inshore water to about 46 m over rock, coral or mud; adults stay near refuges: caves, wrecks and
  ledges [FM]. Solitary, small home ranges, little movement between reefs [FB].
- **Diet:** mainly crustaceans: spiny lobsters, shrimp, **crabs**; also fish (stingrays) and, rarely, octopus and
  young turtles [FM] [FB]. So a 12 cm crab is ordinary prey.
- **Feeding:** it ambushes with a quick rush and a snap; most prey is "simply engulfed and swallowed whole" [FM].
  Groupers in general feed by suction, mouth and gills making a vacuum that pulls prey in [WIKI-G] (a Wikipedia line
  with no citation: treat the suction cone as a faithful reading of the family, not a measurement for this species).
- **Territory:** adults defend the area near a refuge, "displaying an open mouth and quivering body to intruders"
  [FM] [FB]. That display is the tell.
- **Sound:** a loud rumble made by contracting the swim bladder, carrying far underwater [FM]. That it is made when
  threatened or territorial: not verified (the Museum page says it is used to locate other groupers).
- **Not verified:** spitting out struggling or inedible prey; the gulp's duration; any distance for its territory;
  a pull on a 12 cm crab; booms as a warning to crabs. All are game design.

## Role and register
**Chamber guardian** (register 3 of `../world-vision-brief.md`): threatening but alluring. It holds its cave mouth;
the prize lies behind it. **One thing it teaches: suction.** Brace (block) and stay out of the gulp's cone; punish
the slow recovery. It is not parryable (you cannot parry a vacuum); the answer is to leave the cone, dodge through,
or brace and take almost nothing.

How it differs from the README table: the mantis rushes and is parried; the moray ambushes from a crevice and holds
on; the triggerfish flips you with a jet and bites. The grouper never chases: it makes you come to it, warns twice
(a boom, then a slow gape) and its blow is an area (a cone) rather than a point. It is the first creature that
moves you without hitting you.

## Combat design
- **Boom (warning, no damage):** a crab inside 700 units of the cave mouth makes it face you and boom (State
  `Notice`, 1.5 s, phase `boom`; the body quivers, pectorals flutter). If you stay in its territory but out of
  reach it waits 5 s, stands down, and booms again when it next notices you. Nothing hurts during a boom.
- **Tell (1.2 s, phase `gape`, last 0.3 s `locked`):** gill covers flare and the lower jaw drops slowly; the head
  tilts to aim at you. It tracks you until the last 36 ticks, then the aim is locked. A `Cue::Tell` swell plays.
- **Gulp (12 ticks):** a **suction cone** ahead of the mouth, 460 units long, half-angle 25 degrees. A crab in it is
  pulled toward the mouth (stronger the closer, 3,600 u/s at the mouth falling to half at the far edge; a crab at
  250 units is dragged about 220 units). The pull sets the crab's own velocity (only on the crab's own device) and
  stops it steering (2 ticks of hit stun).
- **Engulf:** a crab inside the mouth circle (radius 120) in the gulp's last 3 ticks is **engulfed**: 6 damage,
  grabbed, held at the mouth. It is bitten 6 more every 30 ticks but **never more than 30 in all**; the hold lasts
  at most 1 s (120 ticks), and each dodge press takes 30 ticks off. Then it is **spat out** ahead of the mouth
  (500 u/s) and the fish gapes in recovery. (Spitting is a design call, not verified.)
- **The crab's answers:**
  - *Leave the cone:* step back or sideways before the lock ends; a crab outside the cone is not pulled at all.
  - *Dodge through:* invulnerable on ticks 2 to 13: pressed on the tell's last tick it covers the whole gulp and the
    engulf test; you end up at its mouth with it recovering.
  - *Brace (block):* pulled only 25%; a braced crab at the mouth is "Blocked": a chip of damage (x0.2) and poise
    damage, no engulf.
  - *Punch the lip (design call):* a crusher smash that lands on the mouth during the gulp's last 10 ticks
    staggers it (90 ticks), cancels the gulp and opens the counter window (x2 crusher). Not a parry (it is not a
    blow); it is the claw meeting the opening jaw.
  - *Punish the recovery:* 180 ticks (1.5 s) after every gulp with the head low: head hits count x1.5.
  - *Climb or shelter:* it breaks off for a crab holding coral or kelp, or in a rest crevice.
- **Weaknesses (from biology):** a slow, telegraphed attack; a huge mouth that cannot turn quickly (a crab behind its
  mouth line cannot be gulped, and it turns about only after 0.75 s of you being behind it); it wants a meal, not a
  fight: hurt, it withdraws.
- **Flanks are safe by design, and that is a known weakness:** a crab beside the body, behind the mouth, is never
  in the cone, so patient flank hits can wear it down (about 35 smashes and its poise breaks every 8). The retreat
  at half health and the departure at 15% are what the fight is for; a "tail slap" or sideways gulp would need new
  biology to justify.
- **Feel:** cozy: a huge patient neighbour parked at its door; brutal: the water lifting you off the floor toward a
  mouth; surreal: a fish that inhales the sea.

## Implementation spec
| State (`enemy::State`) | Ticks | Phase | Notes |
|---|---|---|---|
| Idle | | `guard` (`backed` while resting) | Holds station at home, 90 u/s to correct; notices a crab within 700 of the cave mouth |
| Notice | 180 | `boom` | No damage. Then Approach |
| Approach | up to 600 | `alert` | Holds station, turns about after 90 ticks of the crab being behind it. Starts the tell when the crab's middle is within 420 of the mouth, ahead of it, between 40 degrees above and 60 below the facing axis, with a clear line |
| Tell | 144 (last 36 locked) | `gape`, `locked` | Aim tracks the crab, then locks |
| Attack (gulp) | 12 | `gulp` | Pull in the cone; lip check on ticks 3 to 12; engulf check on ticks 10 to 12 |
| Grab (engulfed) | <= 121 | `engulf` | 6 every 30 ticks, 30 max in all (the first 6 included) |
| Recover | 180 | `recover` | Head x1.5 |
| Stagger | 120 (poise) / 90 (lip) | `stagger` | |
| Flee | up to 600 | `back` / `leave` | See below |
| Gone | 43,200 | | Leaving: hidden, another occupies its place later. Killed: body lies belly up |

- **Hurt:** the generic rule sends it into `Flee` below 50% health (once per fight). It **backs 280 units into its cave** and
  then **stops guarding for 60 s** (`backed`: it notices nothing: the prize window). A blow while it rests ends the
  rest at once (it booms again). A stagger does not cancel a retreat in progress. Below 15% it **leaves**: swims off
  at 200 u/s for 6 s, then is gone for 6 minutes. Left alone and unhit for 20 s with nobody near, it mends fully and
  may back out again. It never moves from home except to back out or leave. It never enters tunnels (it is a
  non-burrower: the shared sanctuary guard applies) and a crab inside a tunnel is invisible to it.
- **Boxes:** hurtbox 1,250 x 420 centred on home x, from home y upward; mouth point 600 ahead and 180 up from
  its feet point; mouth circle r 120; lip circle r 100.
- **Numbers:** health 400, poise 200 (refill 20/s after 90 ticks; stagger 120). Crab smash 12 damage / 25 poise: 34
  clean smashes or 8 poise-breaking ones. Engulf 6 + up to 4 bites of 6 = **30 maximum**; 20 poise on the engulf. No
  other damage exists: **no blow, no combo.** A full 100-shell crab is never killed by one gulp (it takes 4 gulps
  undefended, each with a 2.7 s warning and 1.5 s recovery).
- **Real vs tuned:** size is real (sources above). Everything else, including all timings, the cone, the pull, the
  territory and the 60-second rest, is tuned for play.
- **Statuses used:** existing `grabbed` (mash dodge to free; autotomy still works), `knocked` (2 ticks, no steering
  during the pull), crusher `counter` window after a lip hit. No new status.
- **Drops (`DROPS`):** none; it is driven off, not eaten. A killed grouper lies where it fell.
- **Fed:** after it spits a crab out it will not gulp again for 6 s, so a crab can walk on past.
- **Eats:** crabs, lobsters, fish. It will gulp a crab it can reach and spit it out (design call).

## Where it lives and the encounter
Open water on the eastern reef (Silt Hollow to Coral Rise), hanging over the flat seabed east of the coral stand at
x 16220, its mouth facing west, the way in. `spots` anchors to the landmark **"coral stand 20"** by name (its right
edge + 20) and takes the height from `level.floor_height`: the mouth hangs 300 above the floor a crab stands on
300 units in front of it (`meeting_height`), but never lower than 100 over the floor under its middle
(`hover_height`, the authored height: on today's terrain the flat seabed puts it about 260 units above the
slope's foot, so the way in is to climb the bank toward its mouth). Its notional cave is the rising bank behind it. **Rule broken, on purpose:** the brief asks
for 400+ units from every refuge, but a scan of the whole eastern reef found no spot that is both 400 from every
coral stand and clear of the other residents (the reef has a coral stand about every 500 units; the moray's crevice
is at x 15220 and the mantis at 16800; an earlier spot at x 15820 put its mouth on the moray, which killed the
scripted crab of `eastern_upper_route.rs`). So it sits at a coral head, between stands 20 and 22; the mouth is 480
from the moray and the cone (460) stops short of it. A test checks no other resident is inside the cone's reach of
the mouth and that it is not inside a refuge, tunnel, mouth or chamber. It keeps out of every named pocket and
tunnel. A crab holding coral is left alone, so a retreat is always in sight.
- **Prize for the skills/world build:** `grouper::prize(level, home)`: on the floor 300 units behind the mouth
  (home x - 300), in plain sight of the way in and well within 800 units of it. No prize object is made here. A crab
  at the prize is inside its territory, so take it during the rest.
- **One threat per chamber:** it is the only one in reach.
- **Phases of the fight:** (1) boom and gulps while it guards; (2) at half health it backs into the bank and rests
  60 s; the prize is free; hitting it wakes it; (3) at 15% it leaves.
- **World build:** all positions come from landmark names and the floor; if "coral stand 19" or "20" are gone,
  `spots` returns nothing. It stays out of the pit (vanishing landmarks), the home burrow, the clam beds and the
  other named pockets.

## Rig notes
Parts (`crates/crabrig/src/bestiary/grouper.rs`, 9 exported): body (root; with belly, gill and mottling), lower
**jaw** and **mouth-in** (the dark red inside, pivot at the mouth's corner), **operculum** (the gill cover, pivot at
its top; flares back over the dark gill), pectoral (with rays), tail, dorsal, anal, eye (pupil, glint). Ink only.
Poses read only from the sim: **idle** (tail and fin sway), **boom** (quiver, fast pectoral flutter), **tell**
(jaw dropping, cover flaring, head tilting to the aim, a third of its angle), **gulp** (jaw wide, cover wide),
**engulf** (jaw shut), **recover** (jaw closing slowly), **hurt**/**stagger** (wobble, jaw ajar), **defeat** (belly up
on the floor, jaw ajar), **leave** (tail beats; gone and hidden).

## Scenario tests (`crates/sim/tests/goliath_grouper.rs`)
Placement (in `Level::test_level()`'s foes, 400+ from every refuge, tunnel, mouth and chamber); the boom is first
and hurts nothing; the tell is 144 ticks with a `Cue::Tell`, the gulp 12; the pull happens only in the cone and a
brace cuts it; an engulf is held, costs at most 30 and spits out; struggling frees sooner; walking out of the cone
takes nothing; a dodge through takes nothing; a braced crab is not swallowed; a lip smash staggers it; poise breaks;
head hits count more in recovery and the recovery can be punished; a crab can drive it off and it never strays;
hurt past half it backs out and rests 60 s; a hit wakes it; walking away ends it; it breaks off for a crab holding
coral; it never follows into a tunnel; no engulf exceeds 30; determinism (two sims, same hash each second); a
remote crab is never changed; co-op (the guest defends once, both devices agree); phase names.

## Sources
- [FM] Florida Museum, Atlantic goliath grouper species profile (opened). <https://www.floridamuseum.ufl.edu/discover-fish/species-profiles/epinephelus-itajara/>
- [FB] FishBase, *Epinephelus itajara* (opened). <https://www.fishbase.se/summary/Epinephelus-itajara.html>
- [WIKI-G] Wikipedia, "Grouper" (start point; the suction sentence is uncited there). <https://en.wikipedia.org/wiki/Grouper>
