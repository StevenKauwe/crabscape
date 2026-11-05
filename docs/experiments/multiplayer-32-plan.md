# 32 total players: implementation and acceptance plan

Status: design and diagnostic only, based on main abe78e7 (2026-11-05). This document does not claim 32-player support. Target means one owner plus up to 31 visitors in one world. No old-protocol compatibility is required; every participant must run the same build.

## Evidence and current limits

- Three isolated Chrome profiles on production: first visitor joined; second visitor repeatedly received `This session already has two players; your solo world is kept`. No browser errors. Evidence retained locally in `/tmp/crab-three-player.log`.
- `src/coop/web/cells.rs`: one `peer`, `guest`, remote control state, transfer receiver and acknowledgement stream. The receive loop sends `Wire::Busy` to another guest. Removing that rejection would mix identities and transfer state.
- `src/coop/web/cells/regional.rs`: one remote journal, guest position, remote bounds, backup and transfer. `grant` partitions only HOST and one visitor. Recovery and save ownership rely on this structure.
- `crates/sim/src/world/cell_session.rs`: admission, snapshot actor list and other-crab validation capped at 8. `world/regions.rs` limits rosters, control deserialization and frames to 8; `world/regions/journal.rs` limits journal rosters to 8. `cell_interest::MAX_PLAYERS` is 64: this is a packet/interest bound, not evidence of playable capacity.
- The legacy `CellSessionTick`/recording and guarded two-player stepping APIs must not accidentally become the N-player live path. Use regional frames, with explicit rosters.
- `tools/introducer` uses Matchbox full-mesh signaling. MAX_ROOM=80 bounds the room NAME, not player count. `rooms.rs` counts connections but does not impose a gameplay capacity. fly.toml configures a shared CPU and 256 MB, sleeping when idle. Deployed machine size and real capacity have not been independently queried or load-tested.
- At 32 participants, full mesh has 496 peer pairs, 31 data peers per browser. Voice has its own peer connections. This is quadratic setup/transport cost even if gameplay messages are routed only to an authority.
- Existing cellular browser tests primarily exercise two players; three-player legacy lobby tests do not validate the cellular protocol at 32.

## Ownership boundary, inspected read-only

Do not edit Claude's original worktrees or integrate their uncommitted files.

| Work | Exact overlap and implication |
| --- | --- |
| trigger-overturn | Dirty `crates/sim/src/world.rs` changes material controls and movement when overturned; also combat.rs, enemy.rs, triggerfish.rs, tuning.rs, src/fight.rs, src/puppet.rs. Preserve these rules when wiring simulation ticks. |
| sound / PR493 | world.rs adds sound-cue state and hooks around enemy updates and tick completion; src/sound.rs and src/settings.rs are also owned. Network replay must preserve cue semantics, and voice topology changes must coordinate with this owner. |
| readable-hints | Dirty crates/sim/src/lib.rs, src/main.rs, src/go_home.rs and new tidepool modules. Avoid registration/export changes there. Home actions must remain correctly routed. |
| buried-clam-bed | Dirty level.rs, world.rs, src/fight.rs, src/food.rs and clam modules. Resource authority/recovery must not duplicate rewards. |
| cell-crossing-compute | Reserved underwater.rs, active_remap.rs, background_waves.rs, chunk_flow.rs, sleeping_archive.rs and related terrain work. Consume existing terrain APIs; do not redesign remapping while generalizing network ownership. |
| lantern / PR419 | physics.rs, enemy.rs, memory.rs, tuning.rs, world.rs, progress/replay changes and presentation files. Do not bundle sight or movement changes. |

No inspected dirty patch owns the proposed new coordinator module, session-capacity example, `src/coop/cells_wire.rs`, `src/coop/web/cells.rs`, `src/coop/web/cells/regional.rs`, or `world/regions/journal.rs`. Those are a viable implementation boundary, but their interfaces touch the protected simulation and terrain behavior. A blanket claim that all networking is blocked would be inaccurate. Recheck ownership before implementation.

## Architecture decision to prototype

Use an owner-coordinated, interest-filtered network, with one authority per active region. Keep the existing deterministic regional simulation; each region orders its roster's inputs, not all 32 participants globally. Independent regions must keep advancing when another guest stalls.

1. Extract pure coordinator state into a new Rust module, independent of Bevy/browser APIs. Key participants by PlayerId and authenticated connection; track incarnation/epoch, input sequence, transfer IDs, applied acknowledgements, timeout state and rate limits separately. Admission is transactional: reserve a slot, validate bounded progress, transfer checkpoint, receive readiness, then publish roster membership at a tick boundary. Reject participant 33 cleanly and release abandoned reservations.
2. Replace the single-guest regional model with maps keyed by RegionId and PlayerId: region membership, authority, journals, transfer/recovery and subscriptions. Merge/split based on overlap of player interest, preserving deterministic ordering and NPC identities. Transitive overlap can join many players into one region; test this worst case explicitly. Do not assume two disjoint rectangles or one remote owner.
3. Use bounded N-player RegionFrame controls and sequenced actions. Validate identity against the sending connection, membership and epoch; reject duplicates, stale frames and oversized payloads. Only change the 8-player limits when the new coordinator and tests can use the larger roster. Keep independent limits for actions, bytes, queued frames, checkpoints and actors.
4. Route only needed regional inputs/state. Serialize a canonical frame once and share its bytes across recipients. A slow guest gets bounded backlog then resynchronization/disconnect; it cannot indefinitely stall unrelated regions. Do not invent missing canonical inputs silently. State the timeout/held-input policy explicitly in protocol tests.
5. Plan sparse data connections (owner star plus region-authority subscriptions), rather than retaining 496 links as the final design. Verify whether Matchbox's signaling topology/socket layer can negotiate this; modifying a send loop alone does not remove full-mesh peer connections. A host star reduces idle links to 31 but concentrates upload and recovery responsibility. Measure before selecting host-star-only versus additional region-owner links.
6. Voice is a separate capacity decision. Preserve existing behavior for small rooms while testing active-speaker/proximity subscriptions. A general 32-way voice mesh is not an acceptable unmeasured default. An SFU would require new hosting/cost/operations decisions and is not implicitly authorized by this plan. Never label data-only support as 32-player voice support.

## Recovery and durable state

- Each region has an epoch and a bounded canonical journal; checkpoints name their roster, committed tick, terrain/resource revision and authority. Transfer completion is atomic and idempotent.
- Leaving/timeout removes one actor at a canonical boundary. Join/leave during transfer must cancel only that participant's transfer and release buffers/slots.
- Authority loss elects a successor using the same validated roster/epoch across survivors; restore the last acknowledged checkpoint/journal before advancing. Split brain must reject old authority messages.
- Owner loss must define who retains the durable world and who becomes save publisher. Preserve existing standalone recovery. Test resource receipts, food, drops and terrain across handoffs, not only actor positions.
- Saves do not need compatibility migration. Failed writes retain the last complete save and retry; incompatible loads must reset cleanly with current saving enabled.

## Test ladder and release gates

A green lower tier does not imply a higher tier passed. Use deterministic seeds, bounded runtimes and isolated stores. Keep traces on failure.

1. **Current capacity audit**: `cargo run -p crabsim --release --example session_capacity -- 32`. Attempts all 32 admissions, round-trips the admitted roster, applies 120 identical mixed-control ticks to two copies with hash comparison, then alternates leave/rejoin and round-trips again. Currently expected to fail the requested capacity. `-- 8` is a baseline control. Timing includes two simulations and hashing; it is not FPS or network latency.
2. **Pure coordinator simulation**: 3, 8, 16, 32 participants, plus rejected 33rd. Fake transport with latency/jitter/drop/duplication/reordering, simultaneous join, reconnect, stale epochs, owner death, partition/reunion and stalled guests. Assert membership, canonical tick/hash agreement, exclusive authority, bounded queues and no duplicate resource ownership. Test one cluster and 32 separated interests.
3. **Real local signaling**: 32 WebSocket peers plus churn against a local introducer; validate notifications, cleanup, admission and bounded memory. This checks signaling only, not WebRTC, TURN, simulation or voice.
4. **Actual browser sessions**: start with three cellular clients as the first regression, then 8 real clients. Exercise movement/digging, region split/reunion, join during save, disconnect/rejoin and owner loss; compare canonical state at synchronization points. All clients must show the same roster, not merely connected WebRTC peers.
5. **32-client soak**: use distributed browser runners if one Mac saturates. Run clustered, dispersed and repeated boundary-crossing routes for at least 15 minutes with automated churn. Collect per-client frame pacing, simulation progress, input latency, heap, connection count, encoded bytes, bufferedAmount, transfer queue depths and recovery times. Distinguish runner saturation from game limits. Run phone + desktop mixed sessions; an M2 desktop pass does not establish phone capacity.
6. **Adverse network**: repeat the join/recovery scenarios at specified RTT/loss and with TURN-required paths. Voice-off and voice-on results reported separately. No blanket promise of arbitrary 32-client Internet performance.
7. **Release**: coherent implementation PR, standard required checks, preview three-player playthrough plus capacity report, protected merge, deployed revision and live three-player smoke. Do not advertise 32 until tier 5 and relevant tier 6 criteria pass. Intermediate releases may improve capacity, but state their verified limits explicitly.

## Initial engineering budgets (proposals to calibrate, not measured guarantees)

- Desktop reference: 60 FPS target; report p50/p95/p99 and maximum frame/tick gaps separately. Target p95 frame <=20 ms, p99 <=33.3 ms, and no unexplained simulation pause >100 ms outside explicit join/recovery transitions. Clustered 32-player scenes must pass, not just empty regions.
- Phone reference: stable 30 FPS battery-saver mode; report physical device and sustained thermal behavior. Avoid flaky wall-clock CI gates; deterministic queue/work-count limits guard regressions, repeated hardware runs establish timing.
- Network: every queue has byte and message caps, with explicit backpressure/recovery. Choose caps from measured 32-player traffic and retain existing checkpoint safety bounds until evidence justifies changes. Report host upload separately; no unsupported bandwidth number is promised.
- Signaling: soak the configured 256 MB class with headroom (proposed <70% sustained memory), no connection leaks after churn and no OOM. Inspect actual deployed sizing before claiming this budget applies to production. Do not resize services or add paid infrastructure as part of the diagnostic.

## Concrete next implementation unit

Build the pure roster/transfer coordinator and its 3/8/32 transport simulations, then integrate a three-player cellular browser path through a new module while preserving the two-player behavior. This is one coherent foundation for N-player play, not a constant-only release. Coordinate any needed world.rs/lib.rs edits with Claude before making them; the inspected browser and regional modules can be worked on independently now. Sparse topology, 32-player load validation and voice remain explicit later gates.

## Diagnostic results from this branch

The release-mode audit at target 32 exited nonzero after admitting 8, as intended. The target-8 control exited successfully. Both passed checkpoint round-trip, 120 paired mixed-control ticks with per-tick state hashes, and alternating leave/rejoin. Initial checkpoint: 81,611 bytes. Paired ticks plus repeated hashing took about 11.0 seconds on this Mac; that is diagnostic wall time, not an isolated simulation/FPS benchmark. It does not establish playable eight-browser performance. Focused example clippy passed. No production networking or capacity limits changed.
