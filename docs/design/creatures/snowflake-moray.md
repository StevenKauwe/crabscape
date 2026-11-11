# Snowflake moray (*Echidna nebulosa*)

Concept sheet: `art/creatures/snowflake-moray/snowflake-moray.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** up to 100 cm, commonly 50 cm [FB-EN]; 75 cm in the Andaman Sea survey [KAH]. Body depth at the gill
  opening is 1/14–1/23 of its length [FB-EN-M]. Game adult: 60 cm (375 units) in the plain test level, but only 3.8 cm
  deep (24 units): a long, thin giant. Head 7 cm long (44 units; not verified). In the game's sea (the cell world) it
  is 5/3 that: **a 100 cm adult**, the largest FishBase gives (head 73 x 40 units, 11.7 x 6.4 cm). Swimming out of
  its crevice it is drawn whole, head and fifteen segments narrowing to the tail (size audit: it was strung only 37
  cm long, its head alone out of the crevice); only the head is its hurtbox.
- **Where:** among rocks and corals on intertidal reef flats, shallow lagoons and seaward reefs, 0–48 m, usually
  above 10 m [FB-EN]. Shallows and reef crevices.
- **Diet:** mainly crustaceans [FB-EN]; snowflake morays usually eat hard-shelled prey such as crabs and have been
  seen lunging at crabs on shore [EUR]. Blunt, molar-like teeth for crushing shells (Wikipedia, citing Randall 2005).
- **Hunting:** nocturnal [FB-EN]; morays rest in crevices by day, sneak through the reef and corner prey in holes
  [BSH06]. Small eyes; they rely mostly on smell (Wikipedia: not verified from a primary source).
- **Second jaws:** after the mouth grabs, **pharyngeal jaws** in the throat launch forward, seize the prey and haul it
  toward the gullet [MW07, EUR]. Once bitten, you are being swallowed.
- **Breathing gape:** morays open and close the mouth constantly to pump water over the gills (Wikipedia, flagged
  there as needing a source: not verified). The tell below is built on it.

## Combat design
**Role: ambusher.** A head that comes out of a wall. Teaches reading stillness and routes past danger.
- **Attacks:** *lunge bite* out of the crevice; on a hit, *grab*: the throat jaws pull, and the crab must break free.
- **Tell:** the rhythmic gape stops, the head pulls back into an S, the mouth clamps shut (design reading of real
  behaviour; the stop-before-lunge itself is not verified). Then the lunge.
- **The crab's answers:** *parry* (a crusher rap on the snout: it recoils, neck exposed); *dodge* out of the lunge
  line; *block* (it bites the raised claws: little damage, no grab); if grabbed, mash dodge or **autotomise** a leg.
  Hiding in the dark doesn't help: it smells you. Moving fast past it, or at midday, does.
- **Weaknesses:** it can't leave the crevice quickly; the neck is exposed while it recovers; by day it stays home.
- **Feel:** cozy: the old neighbour in the wall you learn to nod past; brutal: getting dragged into a hole; surreal: a
  mouth inside a mouth.

## Implementation spec
| State | Ticks | Notes |
|---|---|---|
| Resting (day) | — | In the crevice, head just out, gaping. Notice: scent builds while the crab is within 260 units; notices after 120 ticks of scent (tuned). Light makes no difference |
| Roaming (night) | — | Leaves the crevice and patrols between crevices at 140 u/s |
| Track | — | Head turns to follow; gape continues |
| Tell: pull back | 60 | Gape stops, S-curve, mouth shut |
| Lunge (active) | 8 | Head travels 150 units (2 crab widths, tuned) |
| Grab | ≤150 | 15 damage on grab, then 4 every 30 ticks; each dodge press −8 ticks; autotomy frees at once. Time out: 60 damage and it lets go (tuned) |
| Recover | 50 | Neck extended: neck hits ×1.5 |
| Stagger (parried) | 60 | Snout hit ×2 during it |
| Flee | — | Below 30%: back deep in the crevice for 10 s |

- **Boxes:** head hurtbox 44 × 24; once lunged it stretches out to the head (head and neck are hit); bite hitbox
  circle r 12 (about its gape; tuned) at the snout, 12 up, during the active ticks. The rock shields the rest.
- **Numbers:** health 90, poise 70. Lunge 18 damage, 40 poise (block: ×0.2, no grab).
- **Drops:** food; the crevice it guarded opens as a shortcut or stash spot.
- **Scenario tests:** a parry in the window takes no damage and staggers it; a block takes ~4 and is never a grab; an
  unblocked hit grabs; a dodge press every 4 ticks frees the crab in about 50 ticks; autotomy frees at once and drops
  walk speed 8%; a lantern-off crab is noticed as fast as a lit one; a crab outside 260 units is never noticed.
- **In the sim:** `Kind::Moray` (`crates/sim/src/enemy.rs`, numbers in `tuning.rs`, tests in `tests/creatures.rs`):
  the original non-cellular fixture waits in its crevice, smells a crab after 120 ticks within 260 (light makes no difference), a 60-tick tell, an
  8-tick lunge whose bite circle travels 130 (21 cm, a third of its length; tuned) out from the head, its body box
  following the head out and back in over 24 ticks, an unblocked bite grabs (15, then 4 every 30, up to
  150 ticks; each dodge press takes 8 off), recover 50, parried 60, flees below 30% (once, 3 s). Not yet: night
  roaming tied to a diel cycle, autotomy, the timeout damage. Cellular morays already swim after sensed prey; see the eastern excursions below.

## Eastern reef foraging (cellular game)

The two existing refuges at x 9,880 and 15,220 alternate rest, a short excursion
into nearby hunting water, and a return home. They still follow sensed prey
with the existing notice, tell, bite, and retreat behavior. Scent works while
out, so this is a moving encounter rather than an invulnerable ambient loop.
The first opening moray and the non-cellular combat fixture are unchanged.

The 20-second cycle, six-second outbound window, 280-unit westward reach,
160–180-unit rise, and two-second stagger between refuges are tuned for
readability. They are not measured foraging intervals or a simulated day/night
cycle. Rest resumes at the fixed refuge; injury, hiding and fleeing override
excursions. The higher Coral Rise coral route remains a four-stroke bypass;
walking into the lower hunting water can trigger pursuit. Nearby prey may
interrupt excursions before the player arrives.

FishBase's *Echidna nebulosa* account was opened for this change: it describes
rock/coral habitat, nocturnal activity, mainly crustacean prey, and individuals
seen in the open or with only their heads exposed [FB-EN]. This supports a
shelter-associated animal moving outside its refuge. The exact short circuit
is an authored encounter, not a field observation. The giant-moray cooperative
hunting study [BSH06] concerns *Gymnothorax javanicus*, not this species, and is
not evidence for this circuit or cooperative behavior in snowflake morays.

The simulation reuses serialized Idle state ticks, existing bounded swim
routing, existing slots and the existing swimming puppet. It adds no saved
fields, population, clock, or randomness. Outbound goals and movement keep
the whole body inside the regional owner's active cells and within 400 units
of home; unavailable/blocked hunting water leaves the animal at its refuge.

## Rig notes
Parts: head (upper jaw + lower jaw, pivot at the jaw hinge behind the eye), eye, 2 tubular nostrils, gill-opening spot,
pharyngeal jaw (a part inside the mouth that slides forward), body as a chain of 8–10 segments with a continuous
dorsal fin edge drawn on, tail tip. No pectoral or pelvic fins (most morays lack them; Wikipedia). The dendritic
blotches are painted on each segment so they move with it.
Key poses: **idle** (slow gape cycle ~1 s, nostrils twitch, head sways); **tell** (segments fold into an S, jaw shut);
**strike** (chain straightens, jaw open wide, pharyngeal jaw forward at the end); **recover** (neck out, jaw working);
**hurt** (snaps back into the crevice).

**Puppet (in the game):** drawn by `crates/crabrig/src/moray.rs` from the concept sheet (side view, facing right) into
`art/creatures/snowflake-moray/snowflake-moray-parts.svg`, exported to `assets/creatures/` (`cargo run -p crabrig
--profile art -- parts --creature snowflake-moray`, then `-- export --creature snowflake-moray`). A kit of seven parts:
the head (eye, nostril tubes, gill opening, blotches), its lower jaw hinged behind the eye, the inside of the mouth
(squeezed to the gape so it only shows between the jaws), the throat jaws, and three body segments with different
snowflake blotches. The game (`src/critters.rs`) builds the head at the simulation's reach (what shows is what hits)
and strings seven segments from the back of the head to the crevice along a curve; in the tell the head rears and
tips nose-down, the neck bows up behind it into the crevice, and the mouth shuts. The jaw gapes while it breathes,
drops wide for the strike, and the throat jaws come forward at the end of the lunge and while it holds on. No tail
in the original fixed-crevice pose. Cellular swimming uses the existing articulated neck and tail-clearance checks instead of pinning the body to the opening. Ink style only for now (no oil-painted copy yet).

## Sources
- [FB-EN] FishBase, *Echidna nebulosa*. <https://www.fishbase.se/summary/Echidna-nebulosa.html>
- [FB-EN-M] FishBase, morphology summary, *Echidna nebulosa*. <https://www.fishbase.se/physiology/Echidna_nebulosa>
- [KAH] National Museum of Nature and Science (Tokyo), Fishes of the Andaman Sea, *Echidna nebulosa*. <https://www.kahaku.go.jp/research/db/zoology/Fishes_of_Andaman_Sea/contents/muraenidae/01.html>
- [EUR] EurekAlert (UC Santa Cruz), snowflake morays swallow on land with their throat jaws. <https://www.eurekalert.org/news-releases/606098>
- [MW07] Mehta & Wainwright (2007), "Raptorial jaws in the throat help moray eels swallow large prey", *Nature* 449:79–82 (read via search summary and [EUR]; not opened).
- [BSH06] Bshary et al. (2006), *PLOS Biology* 4(12):e431. <https://journals.plos.org/plosbiology/article?id=10.1371/journal.pbio.0040431>
- Wikipedia, "Moray eel" (start point). <https://en.wikipedia.org/wiki/Moray_eel>
