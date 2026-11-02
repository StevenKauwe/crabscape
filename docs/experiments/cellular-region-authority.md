# Regional simulation authority

Chosen with Astra after the two-player shared-window implementation. This is the implementation contract for independent exploration. The current browser runtime still uses the earlier shared-window session until the new regional runtime passes its browser checks.

## Ownership and persistence

The lobby owner identifies the persistent world and is its online coordinator. Region authority is a temporary simulation job delegated to a connected browser. The owner keeps local durable world state; no persistence server or new paid service is part of this implementation. The existing introducer continues only to connect peers and receive reports.

The lobby closes when its owner disconnects. Guests return to their private solo worlds with personal gains. There is no stand-in lobby using somebody else's world, offline continuation, or divergent-world merge. A future persistence service can store the same versioned region snapshots and ordered updates, with permissions and revision checks added then.

## Simulation

Use the complete gameplay kernel, with explicit variable-size active regions. Do not substitute the incomplete chunk-flow experiment. An active region owns its cells, aggregates and particle state, crabs, NPCs and their random streams. Near players belong to one region. Distant groups can be simulated independently by different browsers.

Region grants are exclusive and carry a new epoch. Active kernels contain only assigned terrain, without copies of the owner's growing sleeping archive. The canonical terrain store maps each kernel's compact pebble IDs into the world's aggregate identities. Commits update only owned cells. Old epochs cannot overwrite newer ownership. Overlapping interests must merge at an agreed tick before both can modify the same area; split/merge uses hysteresis and material padding, not camera visibility.

NPC state and random streams survive extraction, restoration and reunion. Each area's local actors, pickups, combat and creatures step together. Shared-resource commits are scoped to owned objects. Player Home and other distant transitions require coordinated reassignment rather than stepping outside the kernel.

## Delegation and recovery

The authority sends canonical input batches to the owner. The owner can retain a complete starting checkpoint plus an ordered input journal without continuously running distant physics. On a visit, merge, periodic checkpoint or authority loss, deterministic replay materializes the latest acknowledged regional state. Regular local persistence stores these snapshots and journals. The owner acknowledges accepted sequences and bounds each outstanding journal.

A handoff is a tick-boundary transaction: finish at the agreed cutoff, retain the old checkpoint, transfer and validate complete state, then activate the new epoch. Other regions keep running. Stale-epoch packets are rejected. A region's authority leaving while the owner remains triggers recovery on the owner. Owner loss closes the session; unreceived updates cannot be promised.

## Verification required before enabling

Preserve legacy solo saves and default-kernel physics. Test disjoint modifications followed by reunion, aggregate ID remapping and motion, current/flow/silt continuity, NPC identity and random-stream continuity, independent distant gameplay, transactional rejection of bad input batches, and recovery from acknowledged input history. Then run real browsers through separation, reunion, delegate departure, owner departure and save/reload. Include three players with two nearby and one distant. Compare replicas of the same regional epoch/tick and native/Wasm replay; separate regions intentionally have different world-state hashes.
