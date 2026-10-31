# Ocean-shaped

Everything is biologically true, and the surreal comes from the real ocean. Creatures, dangers and interactions work
the way they actually work. The goal: a marine biologist saying "no way!" at a detail we included.

## The line we walk
- **Real.** The world, the creatures and the dangers behave as they really do.
- **Surreal** = the real ocean seen at crab size. We never have to invent strangeness.
- **Fantastical but grounded.** Our crab does things no single crab does, but that fit how things work: it farms
  glowing bacteria for a lantern, uses human litter (glass, steel, plastic) as tools and armour, and combines
  behaviours from several crab species.
- **Never** break our own rules. Game "magic" exists, but every power comes from a real mechanism and works the same
  for everyone, player included. **Self-consistency is the one rule that never bends.**
- **Art is stylised for emotion;** behaviour and anatomy stay accurate even when the drawing isn't realistic.

## Things the rule rules out
- **No air.** A crab breathes water with its gills. No air tank, air bladder, "brewing air", AIR gauge. The real
  limits are low oxygen (the oxygen minimum zone), cold, pressure and darkness.
- **No pressure helmets as armour.** Deep animals cope with pressure through body chemistry (e.g. the osmolyte TMAO),
  not armour. Going deeper means adapting over time: eating deep food, building tolerance.
- **No fire.** The lantern is alive: a jar of farmed luminous bacteria (a yellow, *Vibrio fischeri* Y-1-like strain;
  sourced in `art/crab/anatomy-notes.md`). It needs oxygenated water to glow.

## Abilities and status effects → real mechanism
| Game term | Real mechanism | Example animals |
|---|---|---|
| Poison / venom | Toxins delivered by stings, spines or bites | Cone snails, box jellyfish, lionfish |
| Stun / shock | Electric discharge | Electric rays (electric eels are freshwater, so any eel-like cousin must fit our sea) |
| Blind | Ink cloud, or a sudden flash of light | Squid and octopus ink, bioluminescent bursts |
| Invisibility | Active camouflage, or counter-illumination that erases your silhouette from below | Cuttlefish, octopus, hatchetfish |
| Shockwave | Cavitation bubble from a claw snapping shut | Pistol shrimp |
| Armour break | A strike fast and hard enough to crack shells | Mantis shrimp |
| Escape and regrow | Autotomy (dropping a limb), regrowing it over the next moults | Crabs themselves |

The rule cuts both ways: if ink blinds a predator, it blinds the player too. If venom works through a sting, a hard
enough shell blocks it.

## Real behaviours that suggest mechanics
**Status: TO VERIFY WITH SOURCES.** Rows without a source each need a real, opened source added
(see the research issues) before it becomes lore or a mechanic. Write sources as `[KEY] author, title, link`.

| # | Real biology (unverified) | What it could become | Source |
|---|---|---|---|
| 1 | Hermit crabs form **vacancy chains**: around an empty shell too big for them, they wait and queue from largest to smallest, then swap shells in sequence, each into the one just left. In a *synchronous* chain the whole line "fires off in seconds" after hours of queuing (*Coenobita clypeatus*); chains can also be asynchronous, one crab at a time over days. Oh no way: waiting crabs **piggyback**, riding on a bigger crab's shell | Shell trading; the home-growing economy | [TUFTS-VC], [WP-VC] |
| 2 | Crabs **moult** and stay soft and vulnerable until the new shell hardens: blue crabs are "particularly susceptible when they are soft" and prefer seclusion; the shell is parchment-like after a few hours and fully hard in two or three days ([CQ]: about four days). Oh no way: the moult itself takes only minutes, and the limp, wrinkled crab swells 25–35% bigger before hardening | Growing up is a risk: hide somewhere safe while soft | [SCDNR-BC], [CQ] |
| 3 | **Red light is absorbed first** with depth, so red animals look black in the deep: at 100 m no red light gets through, and a red fish "appears blackish because there is no red light to reflect". Oh no way: blue animals are rare in the midwater, because blue light reaches deepest and would show them up | Your red crab is camouflaged down there | [NOAA] |
| 4 | **Stoplight loosejaws** (*Malacosteus niger*) make far-red light (over 700 nm) from a photophore under each eye, and see it: two red-shifted visual pigments, boosted by a chlorophyll-derived photosensitizer that most likely comes from the copepods they eat. Most deep-sea fish see only short (blue) wavelengths [WP-ML]; that most deep-sea *animals* can't see red is not verified. Oh no way: the loosejaw's lower jaw has no floor, no skin between its sides, so it can snap shut fast on big prey | A red lantern as late-game stealth gear | [DOUGLAS-2000], [WP-ML] |
| 5 | The **alarm jellyfish** (*Atolla*) flashes when attacked, drawing bigger predators onto its attacker | A defence trick, or a danger when you grab the wrong jelly (`creatures/alarm-jelly.md`) | [MBARI] MBARI, "Deep-sea crown jelly", https://www.mbari.org/animal/deep-sea-crown-jelly/ |
| 6 | **Boxer crabs** carry small anemones in their claws as stinging weapons | Living tools and weapons instead of metal swords | — |
| 7 | **Decorator crabs** stick sponges and algae onto their shells for camouflage | Cosmetic and functional costumes from biology | — |
| 8 | **Yeti crabs** at hydrothermal vents farm bacteria on their hairy claws by waving them | Food farming in the deep | — |
| 9 | **Whale falls** pass through stages over years: scavengers → bone-eating *Osedax* worms → bacteria | The whale-fall village changes over time | — |
| 10 | **Brine pools** on the sea floor are hypersaline lakes that kill animals that wander in | A real environmental hazard | — |
| 11 | **Diel vertical migration:** each night huge numbers of animals rise to feed, then sink at dawn | Day and night change which creatures are where (`universe.md`) | [WHOI-DVM] WHOI news release (2022), https://www.whoi.edu/press-room/news-release/whoi-leads-multi-ship-study-of-northwest-atlantic/ |
| 12 | The **oxygen minimum zone**, roughly 200–1,000 m in many oceans, holds very little dissolved oxygen | The real breathing limit for a crab | — |
| 13 | Deep-sea animals accumulate **TMAO**, which counters pressure's effect on proteins | Depth tolerance you build by adapting | — |

A research log of papers and sources backs every creature and mechanic. Anatomy for the player crab is already
sourced in `art/crab/anatomy-notes.md`; keep its "not verified" flags honest.

## Sources
Each opened and checked against the row it backs.
- **[TUFTS-VC]** Tufts Now, "Social Networking Helps Hermit Crabs Find Homes" (2010), on Rotjan, Chabot & Lewis,
  "Social context of shell acquisition in *Coenobita clypeatus* hermit crabs", *Behavioral Ecology* 21(3) (2010).
  <https://now.tufts.edu/2010/04/26/social-networking-helps-hermit-crabs-find-homes>
- **[WP-VC]** Wikipedia, "Vacancy chain" (synchronous and asynchronous chains, largest to smallest).
  <https://en.wikipedia.org/wiki/Vacancy_chain>
- **[SCDNR-BC]** J. David Whitaker, South Carolina Department of Natural Resources, "Blue Crab".
  <https://dnr.sc.gov/marine/pub/seascience/bluecrab.html>
- **[CQ]** Chesapeake Quarterly, "Glossary of Blue Crab Biology" (soft crab, paper shell, buckram, hard crab).
  <https://www.chesapeakequarterly.net/V11N2/side4/>
- **[NOAA]** NOAA Ocean Exploration, "Why are so many deep-sea animals red in color?".
  <https://oceanexplorer.noaa.gov/facts/red-color.html>
- **[DOUGLAS-2000]** Douglas, Mullineaux & Partridge, "Long-wave sensitivity in deep-sea stomiid dragonfish with
  far-red bioluminescence: evidence for a dietary origin of the chlorophyll-derived retinal photosensitizer of
  *Malacosteus niger*", *Phil. Trans. R. Soc. B* 355 (2000). Abstract: <https://pubmed.ncbi.nlm.nih.gov/11079412/>
- **[WP-ML]** Wikipedia, "Malacosteus niger" (photophore, no mouth floor; "most deep-sea fish have a single visual
  pigment maximally sensitive at short wavelengths", citing Douglas et al. 2016, *Scientific Reports*).
  <https://en.wikipedia.org/wiki/Malacosteus_niger>
