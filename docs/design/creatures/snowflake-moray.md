# Snowflake moray (*Echidna nebulosa*)

Concept sheet: `art/creatures/snowflake-moray/snowflake-moray.png`. Shared rules: `README.md`.

## Biology that matters
- **Size:** up to 100 cm, commonly 50 cm [FB-EN]; 75 cm in the Andaman Sea survey [KAH]. Body depth at the gill
  opening is 1/14–1/23 of its length [FB-EN-M]. Game adult: 60 cm (375 units), five crab shells long but only 3.8 cm
  deep (24 units): a long, thin giant. Head 7 cm long (44 units; not verified).
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
  waits in its crevice, smells a crab after 120 ticks within 260 (light makes no difference), a 60-tick tell, an
  8-tick lunge whose bite circle travels 130 (21 cm, a third of its length; tuned) out from the head, its body box
  following the head out and back in over 24 ticks, an unblocked bite grabs (15, then 4 every 30, up to
  150 ticks; each dodge press takes 8 off), recover 50, parried 60, flees below 30% (once, 3 s). Not yet: night
  roaming, autotomy, the timeout damage.

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
(it never leaves the crevice yet). Ink style only for now (no oil-painted copy yet).

## Sources
- [FB-EN] FishBase, *Echidna nebulosa*. <https://www.fishbase.se/summary/Echidna-nebulosa.html>
- [FB-EN-M] FishBase, morphology summary, *Echidna nebulosa*. <https://www.fishbase.se/physiology/Echidna_nebulosa>
- [KAH] National Museum of Nature and Science (Tokyo), Fishes of the Andaman Sea, *Echidna nebulosa*. <https://www.kahaku.go.jp/research/db/zoology/Fishes_of_Andaman_Sea/contents/muraenidae/01.html>
- [EUR] EurekAlert (UC Santa Cruz), snowflake morays swallow on land with their throat jaws. <https://www.eurekalert.org/news-releases/606098>
- [MW07] Mehta & Wainwright (2007), "Raptorial jaws in the throat help moray eels swallow large prey", *Nature* 449:79–82 (read via search summary and [EUR]; not opened).
- [BSH06] Bshary et al. (2006), *PLOS Biology* 4(12):e431. <https://journals.plos.org/plosbiology/article?id=10.1371/journal.pbio.0040431>
- Wikipedia, "Moray eel" (start point). <https://en.wikipedia.org/wiki/Moray_eel>
