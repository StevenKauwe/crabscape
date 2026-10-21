# The crab's memory (zooming out)

Zoom out as far as you like (pinch, mouse wheel, `-`/`=`), but beyond what the crab perceives you see its memory, not
the world. Rules: `crates/sim/src/memory.rs` (numbers in `Tuning::memory`); drawing: `src/memory.rs`.

## What the crab perceives now
- A circle 360 units (58 cm) round its eyes, wider with each culture in its lantern (+40). It covers the usual view, so
  the usual zoom looks as before. Tuned: the lantern lights only ~18 cm; beyond that the crab gets dim shapes.
- Crab vision is limited and topographic: what a crab can perceive bounds what it can know and care about [ZH06].
  Deep-sea crab eyes are tuned to blue light, likely for spotting bioluminescence [FJC12, not verified].

## What it remembers
| Rule | Biology | Number (tuned unless marked) |
|---|---|---|
| A place is learned over repeated visits | Shore crabs ran a 5-turn maze with dead ends once a week; time to the food fell from 435 s to 68 s over 4 runs, with no errors only from run 3 [DAV19] | Clarity 0.55 after 1 visit, 0.8 after 2, 1.0 from 3. A visit counts after 30 s away |
| Memory fades but keeps a core | Two weeks without the maze, every trained crab still reached the end without food, more slowly (276 s); untrained crabs mostly didn't [DAV19] | Fades towards 35% of what was learned; the rest halves every 3 min per visit (weeks squeezed into minutes) |
| Home is always known | Fiddler crabs forage up to 2 m from the burrow and home by path integration [LBD03, abstract only] | The home shell never fades |
| Animals out of sight are where they were last seen | Design reading of the above | A faint shape at the last sighting, fading like a place seen once; forgotten if the crab sees the spot empty |
| The world as last seen | Same | A ghost net cut while away still shows until the crab sees it again |

- Never seen = water (fog). Remembered = greyer, sinking into the water, with softer edges as it fades; in sight = true.
- Each crab has its own memory (co-op players don't share it). It is stepped on the sim tick (every 12 ticks, 5 cm
  cells), saved with the character, and left out of the state hash (a replay has no save).

## Sources
- [DAV19] Davies, Gagen, Bull & Pope (2019), "Maze learning and memory in a decapod crustacean", *Biology Letters* 15(10). <https://pmc.ncbi.nlm.nih.gov/articles/PMC6832187>
- [LBD03] Layne, Barnes & Duncan (2003), "Mechanisms of homing in the fiddler crab *Uca rapax*. 1. Spatial and temporal characteristics of a system of small-scale navigation", *J. Exp. Biol.* 206:4413–4423 (abstract snippet via PubMed only). <https://pubmed.ncbi.nlm.nih.gov/14610027/>
- [ZH06] Zeil & Hemmi (2006), "The visual ecology of fiddler crabs", *J. Comp. Physiol. A* (abstract). <https://openresearch-repository.anu.edu.au/items/4b9c6c28-641a-4300-bcdd-49777fb1e1a7>
- [FJC12] Frank, Johnsen & Cronin (2012), "Light and vision in the deep-sea benthos: II. Vision in deep-sea crustaceans", *J. Exp. Biol.* 215:3344–3353 (seen in search results, not opened). <https://mdsoar.org/handle/11603/13394>
