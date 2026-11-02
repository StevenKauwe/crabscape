# Cellular co-op gameplay

The playable cellular path now uses host-ordered deterministic replication of the complete simulation. The earlier actor-free transport probes remain developer experiments; their force-validation and distributed-ownership approach is not the gameplay implementation.

## What changed

Normal co-op (`?boring`) keeps its existing split ownership: each browser owns its crab and the host runs the world. Cellular co-op needs every creature and crab to disturb the same sand and water at the same simulation tick. Both browsers therefore run all actors and the same complete world. The host chooses the canonical controls and actions, including the guest's inputs, and sends four simulation ticks per reliable packet. Cellular material advances once per four simulation ticks as before.

The lobby, saved invite codes, Matchbox/WebRTC connection, version negotiation, player names, text chat, voice calls, microphone controls, transcription, report form and save workers are reused. Cellular rooms have a separate suffix and their invite links explicitly select cellular mode. They cannot accidentally mix the two simulation protocols.

## Joining and playing

A guest imports personal progress and current condition, without copying its private world's resources into the host's world. The host takes one complete compressed checkpoint at a tick boundary: actors, creatures, clock, random generator, terrain, streamed chunks, memory and shared progress. Join transfer is bounded and uses eight outstanding checkpoint fragments. The guest validates the checkpoint before starting. Both sides pause simulation during this transfer.

During play the host pipelines canonical batches without waiting for each tick's round trip. Applied-tick acknowledgements bound the lead to 60 ticks. Inputs preserve one-shot button presses, and crafting, moulting, codes and Home are ordered with the movement inputs. Only the selected WebRTC peer can supply session messages; epochs, ordered sequences, packet size, queue size, checkpoint shape and decompression limits are checked.

There is no full-world clone or full-world hash on each tick. The browser test requests a full hash once at a paused batch boundary to compare both actual browser simulations.

## First playable scope

This is two-player co-op in a shared simulation window. Players stay within 700 horizontal and 240 vertical world units of each other; the UI says to stay within the shared view. A third player receives a clear status and keeps its solo world. Separate distant simulation windows, more players, stand-in hosts and host migration are future work. This is not a claim of parity with every ordinary co-op promise.

The guest retains its original solo simulation in memory. Autosaves during a visit combine its private terrain, clock, position, map memory and world resources with personal gains from the visit. Leaving restores that private simulation with the returning crab's gains and current condition. The host removes the departed local session actor and stale creature references. Disconnect, failed transfer and timeout end the session and restore solo play; reconnect starts with a new checkpoint and epoch.

## Replays and reports

An exact cellular co-op replay needs the complete starting checkpoint, both players' canonical controls and their ordered actions. A single player's ordinary recorder cannot reproduce multiplayer effects. Session recordings use a bounded checkpoint and a compressed sequence of repeated control frames; playback runs the same simulation step, including the shared-view constraint.

F9 and the mobile Replay button export the bounded session recording. Report attachments retain their smaller existing limit, so a complete multiplayer checkpoint may be too large to attach. The report form must say when it omits that replay; the report itself, diagnostics, typed description and optional voice transcript still work. No false single-player replay is substituted for the session.

## Verification

`cargo cooptest --only cells-session --repeat 1` starts two isolated real Chrome browsers and the local introducer. It checks join, both actor controls, digging, jumping, codes, identical full simulation state, names, text chat, report POST construction intercepted locally, private-world return, host reload and guest leave/rejoin. `--voice on` additionally uses the existing fake-microphone two-way audio and host-transcription-role checks. The unchanged normal-mode scenarios still run separately.

Headless tests cover bounded checkpoint decoding, complete actor/NPC continuation, private world ownership and personal progress return. Session replay tests compare serialized playback to the actual complete simulation state. Native and Wasm checks remain required; same-build Chrome agreement alone does not prove cross-platform determinism. Mobile performance remains a practical limit of this first complete-world replication approach.
