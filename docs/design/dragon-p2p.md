# Dragon P2P starting point

Open `/dragon/`, choose **Invite friends**, and send the copied link. Each friend opens that same link, chooses a Rider, picks a mission in **Party**, and taps **Ready**. Existing Riders and saves stay on their own devices, even when two people choose the same save slot. Session slots are separate from saved Rider keys.

The existing Matchbox introducer exchanges connection notes only. The browsers send the mission directly to one another over WebRTC. Dragon rooms cannot meet crab-game rooms or other preview builds. The adapter accepts only exact-build packets and binds authority to the data-channel sender. Host changes use an epoch and a deterministic successor; a paused host keeps authority. A lost connection retries while solo play remains available.

This first version shares missions, positions, spells and combat. Invited friends are trusted to play honestly; source binding is not an anti-cheat system. Gems, crafting and progression remain personal. There is no cloud save, account synchronization, shared economy, voice chat, or new game server. Some restrictive networks cannot establish direct WebRTC connections; this version adds no TURN relay. Reloading preserves the Rider, then Party's Join button or Ready returns to an active mission. Ready also handles a faster screen launching before another screen starts its countdown.

## Validation

- `cargo test -p dragon-peer`
- `cargo clippy -p dragon-peer --all-targets -- -D warnings`
- `cargo clippy -p dragon-peer --all-targets --target wasm32-unknown-unknown -- -D warnings`
- `bash tools/build-dragon.sh` builds just the adapter; `tools/build-web.sh` includes it in the normal site build.
- `cargo run --profile art -p cooptest --bin dragon` starts isolated headless Chrome profiles and a loopback introducer. Build the introducer first (`cargo build --release --manifest-path tools/introducer/Cargo.toml`), or set `DRAGON_INTRODUCER` to an existing binary. Checks cover distinct rooms, same-slot save isolation, readiness, shared world and guest input/attacks, source-bound authority, reload/rejoin and host departure, repeated Party launches and safe return to solo. It never reads an existing browser profile.

Readonly acceptance attributes are enabled only with `?diagnostics`; normal play avoids those per-frame DOM writes. The functional browser fixture uses the existing simple-graphics option, stops rendering its unrelated isolation room, and allows a bounded slow-runner countdown without retries. Its attack check steers toward an observed target through real game controls, rather than hoping a random-height enemy crosses one fixed line of fire.

Browser tests do not establish physical iPhone or adverse-network performance. Keep any capacity claims limited to the browser configurations actually validated.
