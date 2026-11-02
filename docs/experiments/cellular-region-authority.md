# Regional simulation authority

Chosen with Astra after the two-player shared-window implementation. This is the implementation contract for independent exploration. The browser uses regional kernels. Shared play, exact browser-to-native replay, reporting, reload and private-world restoration have passed the real-browser check. Independent exploration and ordinary window moves remain under acceptance testing.

The current browser coordinator admits one owner and one guest. The native kernels support eight actors, but that does not make the browser coordinator support eight players. A third player receives an explicit capacity message and keeps their private world. Full co-op parity requires extending that coordinator and proving the three-player flow below.

## Ownership and persistence

The lobby owner identifies the persistent world and is its online coordinator. Region authority is a temporary simulation job delegated to a connected browser. The owner keeps local durable world state; no persistence server or new paid service is part of this implementation. The existing introducer continues only to connect peers and receive reports.

The lobby closes when its owner disconnects. Guests return to their private solo worlds with personal gains. There is no stand-in lobby using somebody else's world, offline continuation, or divergent-world merge. A future persistence service can store the same versioned region snapshots and ordered updates, with permissions and revision checks added then.

## Simulation

Use the complete gameplay kernel, with explicit variable-size active regions. Do not substitute the incomplete chunk-flow experiment. An active region owns its cells, aggregates and particle state, crabs, NPCs and their random streams. Near players belong to one region. Distant groups can be simulated independently by different browsers.

Region grants are exclusive and carry a new epoch. Active kernels contain only assigned terrain, without copies of the owner's growing sleeping archive. The canonical terrain store maps each kernel's compact pebble IDs into the world's aggregate identities. Commits update only owned cells. Old epochs cannot overwrite newer ownership. Overlapping interests must merge at an agreed tick before both can modify the same area; split/merge uses hysteresis and material padding, not camera visibility.

The frozen initial join offer also sends only the active terrain window. The owner retains its complete sleeping archive, then issues the normal scoped regional grant before gameplay starts. A growing saved world must not be serialized into the initial offer. The 400-chunk browser regression proves large-world save/load, invitation, exact shared state, guest departure and preservation of a distant terrain page.

NPC state and random streams survive extraction, restoration and reunion. Each area's local actors, pickups, combat and creatures step together. Shared-resource commits are scoped to owned objects. Player Home and other distant transitions require coordinated reassignment rather than stepping outside the kernel.

## Delegation and recovery

The authority sends canonical input batches to the owner. The owner can retain a complete starting checkpoint plus an ordered input journal without continuously running distant physics. On a visit, merge or authority loss, deterministic replay materializes the latest acknowledged regional state. The guest also sends a checkpoint every five seconds of simulated time without pausing gameplay. The owner's ordinary local save combines its current region with the latest complete guest checkpoint and sleeping terrain; it does not replay the guest's physics on every autosave. A sudden owner shutdown can therefore lose the uncheckpointed guest tail. The owner acknowledges accepted sequences and bounds each outstanding journal.

A change of authority is a tick-boundary transaction: finish at the agreed cutoff, retain the old checkpoint, transfer and validate complete state, then activate the new epoch. Other regions keep running. Stale-epoch packets are rejected. A region's authority leaving while the owner remains triggers recovery on the owner. Owner loss closes the session; unreceived updates cannot be promised.

Ordinary movement with the same authority and players prefetches only the entering terrain strips and sleeping creatures while the current area continues running. At a complete input-batch boundary, retain live overlapping cells, actors and creature random streams, install the prepared strips, and return leaving state to the owner. Keep the same epoch and advance a separate window revision. The boundary event belongs in the replay journal at its exact tick; a checkpoint at that tick must also identify its revision. Reserve incoming terrain before transfer so another authority cannot acquire it. If preparation cannot complete safely before the old area's edge, use the coordinated handoff above.

Commit each accepted guest backup to the canonical archive once, reusing the state decoded for validation. An older backup may still advance the replay journal while retaining later boundary events, but it cannot overwrite a newer window revision. Autosaving captures this archive plus the owner's current region; it does not repeatedly decode the same remote backup. Compaction from a live local kernel validates identity, roster and tick without decoding the checkpoint it just encoded.

For a local frame trace, run `CRABSCAPE_FRAME_PROFILE=35 CRABSCAPE_PROFILE_WALK=1 CRABSCAPE_PROFILE_GPU=1 cargo cooptest --only cells-session --repeat 1 --voice off`. This holds the owner still and routes the guest across region boundaries, retains frame/task/operation timing files, and checks both regional replays against native physics. `CRABSCAPE_CPU_PROFILE=1` also retains Chrome CPU profiles. These options are excluded from normal acceptance runs. Hardware graphics avoids confusing software-rendered CI frames with simulation stalls.

## Verification required before enabling

Preserve default-kernel physics and the last complete save on a failed write; do not add compatibility with previous formats. Test disjoint modifications followed by reunion, aggregate ID remapping and motion, current/flow/silt continuity, NPC identity and random-stream continuity, independent distant gameplay, transactional rejection of bad input batches, and recovery from acknowledged input history. Then run real browsers through separation, reunion, delegate departure, owner departure and save/reload. Include three players with two nearby and one distant. Compare replicas of the same regional epoch/tick and native/Wasm replay; separate regions intentionally have different world-state hashes.
