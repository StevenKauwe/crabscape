# Universe

The ocean as real ecology, by depth: what anchors enemies and later levels. Zones, economy and the first loop are in
`world.md`; creatures in `creatures/`. Sources are listed at the bottom; anything without one is marked.

## Scale
Our crab has a **12 cm carapace** (75 world units), so **1 cm = 6.25 units**. Every creature is drawn and simulated at
its real size against that: a snapping shrimp is half a crab shell, a moray is six crabs long, an octopus's arms are
a metre each. The game squeezes vertical distance (a zone is a few screens, not hundreds of metres), but each zone's
light, life and dangers are the real ones for its depth.

## The map by depth
| Zone (game) | Real depth | Light | Cold, pressure, oxygen | Who lives here | Rare resource |
|---|---|---|---|---|---|
| **Sunlit Shallows**: beach hub, tide pools, kelp, warm reef | 0–200 m [WHOI] | Sunlight; reds fade first and are gone by ~100 m [NOAA-RED]. Light-dependent coral reaches ~30–150 m [NOAA-MCE] | Warmest; 1–21 atm (+1 atm per 10 m) [NOAA-P] | Snails, clams, urchins, kelp; snapping shrimp, mantis shrimp, morays, triggerfish, the common octopus (to 200 m) | A big empty sea-snail shell (the next shell-home), guarded in the octopus's den |
| **Twilight Reef**: cold-water coral | 200–1,000 m [WHOI] | No sunlight to grow by; coral eats plankton [LOPH] | 4–12 °C on the reef [LOPH]; 21–101 atm; the oxygen minimum zone lies at about 200–1,500 m, depending on the ocean [OMZ] | *Lophelia* reefs with conger eels, groupers, squat lobsters, crabs, brittle stars; lanternfish and hatchetfish above [LOPH] | Wild luminous bacteria (lantern cultures) |
| **Midnight Zone**: whale-fall village | 1,000–4,000 m [WHOI] | Only living light; sunlight is gone below ~1,000 m [WHOI] | Falls toward 4 °C [WHOI]; 100–400 atm | Alarm jellies (500–5,000 m) [MBARI-AT]; the whale-fall community, hundreds of species on one carcass, among them bone-eating worms with no mouth or gut [MBARI-WF] | Whale bone: oil-soaked, it can feed a community for decades [MBARI-WF] |
| **The Trench**: across the abyssal plain (4,000–6,000 m) to the hadal trench | 6,000–11,000 m, only in trenches [WHOI] | Dark except living light | Near-freezing (not verified); 600–1,100 atm | Snailfish filmed at 8,336 m, the deepest fish seen [UWA]; amphipods (not verified) | Manganese nodules on the abyssal plain, growing under half a centimetre per million years [NOAA-MN]; whatever makes the lights below |

- **Bioluminescence is the deep's light.** About three-quarters of animals over 1 cm seen in Monterey Bay make light
  (Martini & Haddock 2017; read via press coverage, paper not opened: not verified). Wild light is blue-green; only
  our farmed lantern strain is yellow (`art/crab/anatomy-notes.md`).
- **Red is invisible below the reds.** Red animals look black in the deep [NOAA-RED]. That works for the alarm jelly,
  and for our red crab, until its yellow lantern lights it up.
- **Pressure is handled by body chemistry, not armour.** Deep fish carry more TMAO the deeper they live, and the
  amount needed may cap fish at ~8,200 m (Yancey et al. 2014, PNAS; abstract via search, not opened: not verified).
  The crab goes deeper by adapting (`ocean-shaped.md`).

## Rules of the deep (for everyone: the crab, its friends and every enemy)
- **Oxygen.** Everything here breathes water. The oxygen minimum zone is a band of low-oxygen water [OMZ]; in the game
  fights and sprints there drain stamina faster and nothing refills it. Animals
  built for it (the giant red mysid, with huge gills and lots of oxygen-carrying blood [OMZ]) are at home there; the
  crab crosses it quickly or after adapting.
- **Cold.** Below the twilight zone the water sits near 4 °C [WHOI]. Cold makes cold-blooded animals slow (not
  verified as a rule; needs a source before it changes any number): deep creatures are slow, and so is the crab.
- **Pressure.** Handled by body chemistry, never armour or helmets (see above). Going deeper means adapting first.
- **Darkness.** Below ~1,000 m the only light is living light. Anything red is black there, the crab included.
- **Light is a resource and a risk.** The lantern lets the crab see, but it shows the crab to everything that hunts by
  sight. Turning it down hides you (no help against hunters that use smell or touch, like the moray).
- **Moulting.** Crustaceans grow by moulting and stay soft until the new shell hardens [CQ in
  `art/crab/anatomy-notes.md`]. This holds for the crab (issue #62) and for crustacean enemies alike: a mantis or
  snapping shrimp met soft hides rather than fights (design call).
- **Depth is rarity.** Rarer creatures and resources live deeper and in rarer places (a whale fall, a trench).

## Who eats whom (the crab's place)
```
sunlight → kelp, algae, phytoplankton ──→ snails, urchins, clams, oysters ──→ OUR CRAB (snails, clams, carrion)
                                       │                                   └→ also eaten by: mantis shrimp, triggerfish
plankton ─→ alarm jelly (crustaceans)  │
marine snow + the nightly migrants ──→ deep floor ← whale falls feed the midnight zone for decades [MBARI-WF]
OUR CRAB ──→ eaten by: common octopus (its favourite food), moray, triggerfish, mantis shrimp (prefers crabs)
octopus, moray ──→ eaten by: groupers, sharks (moray predators: groupers, barracudas [MORAY-WP])
```
The crab sits in the middle: it hunts slow, shelled things and scavenges, and most things with teeth, clubs or arms
want to eat it. Snapping shrimp are not predators of crabs; they defend burrows the crab wants.

## Day and night
- **Diel vertical migration:** each night trillions of animals rise from the twilight zone to feed near the surface
  and sink at sunrise [WHOI-DVM]. Nights bring twilight-zone animals into the shallows' water column.
- **Shift change on the reef:** morays rest in crevices by day and hunt at night (they join groupers by day only when
  invited) [BSHARY]; the octopus leaves its den at dusk and returns at dawn [ADW-OV]. Mantis shrimp hunt day and night
  [ADW-OS]. The safest time to cross the reef is midday.

## The pull downward
- **Grow:** a bigger, stronger shell-home needs materials from deeper places (`world.md`). Enemies drop real parts
  (club fragments, scale plates); the octopus's den holds a bigger empty shell.
- **The mystery:** lights far down in the trench, seen first through the crack in the far basin. What they are stays
  unexplained for now, and when revealed it must be real biology (bioluminescence, a whale fall, something living).
  The shed tooth bigger than the crab hints at the giants that pass.

## Rules for creatures
1. **Every creature is its real species.** Diet, hunting style, senses, defences and size come from sources; each
   creature page lists them and marks what is unverified. Combat timings are "real" where measured and "tuned" for play.
2. **Tells are real behaviour.** The warning before an attack is something the animal really does (cocking a club,
   opening a claw, rising to spread its web).
3. **The magic table applies to everyone, the crab included** (`ocean-shaped.md`): venom needs a sting or bite and a
   raised claw or hard shell blocks it; ink blinds whoever is in it; a red animal is black in the deep, and so is the
   crab with its lantern dark; autotomy lets the crab drop a leg to escape a grip (regrowth over later moults: not verified, see #62).
4. **Animals want to eat, not to fight.** Most give up when hurt (flee to a burrow, ink and jet). A fight won often
   means a predator driven off, not killed.
5. **Real scale, no giants.** Big means really big (a 1 m-armed octopus), never inflated.

## Sources
- [WHOI] Woods Hole Oceanographic Institution, "Ocean Zones". <https://www.whoi.edu/know-your-ocean/ocean-topics/how-the-ocean-works/ocean-zones/>
- [WHOI-DVM] WHOI news release, multi-ship study of the twilight zone (2022). <https://www.whoi.edu/press-room/news-release/whoi-leads-multi-ship-study-of-northwest-atlantic/>
- [NOAA-RED] NOAA Ocean Exploration, "Why are so many deep-sea animals red?". <https://oceanexplorer.noaa.gov/facts/red-color.html>
- [NOAA-P] NOAA Ocean Service, "How does pressure change with ocean depth?". <https://oceanservice.noaa.gov/facts/pressure.html>
- [NOAA-MCE] NOAA mesophotic coral pages, 30–150 m (search results only, not opened: not verified).
- [LOPH] Wikipedia, "Lophelia" (start point only). <https://en.wikipedia.org/wiki/Lophelia>
- [MBARI-AT] MBARI, "Deep-sea crown jelly". <https://www.mbari.org/animal/deep-sea-crown-jelly/>
- [OMZ] Wikipedia, "Oxygen minimum zone" (start point). <https://en.wikipedia.org/wiki/Oxygen_minimum_zone>
- [MBARI-WF] MBARI, "Whale carcass yields bone-devouring worms" (2,891 m fall in Monterey Canyon). <https://www.mbari.org/whale-carcass-yields-bone-devouring-worms/>
- [NOAA-MN] NOAA Ocean Exploration, "Manganese Nodule" (DeepCCZ). <https://oceanexplorer.noaa.gov/?p=14785>
- [UWA] University of Western Australia, "Scientists break new record after finding world's deepest fish" (2023). <https://uwa.edu.au/news/article/2023/april/scientists-break-new-record-after-finding-worlds-deepest-fish>
- [BSHARY] Bshary et al. (2006), "Interspecific communicative and coordinated hunting between groupers and giant moray eels", *PLOS Biology* 4(12):e431. <https://journals.plos.org/plosbiology/article?id=10.1371/journal.pbio.0040431>
- [ADW-OV] Animal Diversity Web, *Octopus vulgaris*. <https://animaldiversity.org/accounts/Octopus_vulgaris/>
- [ADW-OS] Animal Diversity Web, *Odontodactylus scyllarus*. <https://animaldiversity.org/accounts/Odontodactylus_scyllarus/>
- [MORAY-WP] Wikipedia, "Moray eel" (start point only). <https://en.wikipedia.org/wiki/Moray_eel>
