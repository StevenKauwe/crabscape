# Eastern reef expedition

The octopus chamber now connects to the eastern reef instead of ending the
map. The opening and existing progression pickups keep their positions.
The world extends from 8,800 to 17,400 units across; the additional distance
is playable terrain, not a larger simulation window.

| Area | Route choices | Discoveries and life |
| --- | --- | --- |
| Rubble Banks, x 8,800–12,000 | Climb over the rock cap or dig through its deep sand pocket; continue down the terraces | A buried tooth, shell grit, clams, a gravel patch, scattered burrow defenders and a paired shrimp pocket |
| Silt Hollow, x 12,000–14,900 | Walk into the mud hollow or use the elevated coral shelves | Sheltered clam pockets, sponge skeletons below and above, a lantern culture, a rest alcove, separated predators |
| Coral Rise, x 14,900–17,400 | Follow the rising seabed back out or climb the coral toward the last culture | Another gravel patch, clams, grit, a final high discovery |

The lower route has continuous slopes in both directions. There are two new
rest stops. Coral provides optional height; no extra permanent swim strokes
or new gear gates are added. The cap bypass uses the same sand and digging
physics as the rest of the game: there is no dig progress meter or scripted
teleport. It contains more sediment depth to leave room for the body and
displaced grains. The wider map uses the existing bounded terrain streaming,
saved chunks, replay inputs, and regional co-op ownership.

Ten habitat occupants are added, using existing animals. They are distributed
in pockets with quieter travel between them, rather than an enemy every few
steps. Mantis shrimp remain separate territorial encounters. The habitat
inspiration is their use of burrows near reef bases in sand and gravel and
their solitary territorial behaviour ([Animal Diversity Web, opened
2026-11-03](https://animaldiversity.org/accounts/Odontodactylus_scyllarus/)).
The map's distances, substrate boundaries, population sizes and replacement
times are game design, not measured biological rates. Its compressed vertical
layout does not assert literal ocean-depth ranges for these animals.

Vacant habitats remain depleted for active simulation minutes: snapping
shrimp 3, mantis shrimp 4, morays 8, octopus 3. Afterward, replacement waits
until every living crab in the owning simulation is over 900 units from both
the habitat and the old body. A bounded region cannot respawn an animal in
another region. Sleeping regions pause their creature timers; this does not
implement reproduction, migrating populations, or wall-clock catch-up.

This is one expanded expedition with existing movement, excavation and
combat. It is not a claim of five hours of finished content.

## Silt Hollow gallery

One of the exposed discovery shelves is now a roofed gallery in a local
seabed depression, between x 14,000 and 14,800. Its two recessed mouths and
sloping floor permit an ordinary walk in either direction. The lantern
culture retains its existing pickup index, but sits on a raised interior
shelf behind the entrance lip: walking through below does not take it.
Existing coral reaches the find without equipment or extra swim strokes.
The exterior coral provides a return over the roof to the other mouth.
The nearby rest stop follows the lowered floor; the opening progression
and the remaining discovery locations keep their identities.

This is a local response to player feedback
[#304](https://github.com/StevenKauwe/crabscape/issues/304) and
[#305](https://github.com/StevenKauwe/crabscape/issues/305), asking for finds
inside wall caves rather than obvious protruding ledges. It does not
replace every exposed shelf or resolve that feedback across the whole map.
