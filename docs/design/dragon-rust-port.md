# Dragon Rust port: work in progress

This branch is an additive port, not a replacement for Dragon. The existing
`web/dragon/index.html`, browser save keys and public game remain unchanged. The PR's
web preview is still the existing JS game, not the native Rust prototypes.

## Run

- `cargo run --bin dragon_flight`: Bevy flight/combat practice range. WASD/arrows steer,
  Space/Enter fires, 1–6 select breath, G changes stage, B changes breath skill, F uses
  a weapon, P changes rider path, Escape pauses, R resets. These are test controls,
  not the game's unlock rules.
- `cargo run --bin dragon_flight -- --save exported-rider.json`: explicitly supplied,
  read-only v3 export preview. No automatic save discovery or writes.
- `cargo run --bin dragon_mission -- --mission m1`: integrated solo mission prototype.
  All 24 base missions and their m1c–m24c Champion variants are supported; Space fires, WASD/arrows move, Escape pauses,
  Backspace restarts. F triggers specials, B/J Daddio powers, R/E/T/Y known-word
  slots, 1–5 select a dragon, Q/Tab swaps. All use a deterministic test seed.
- Add `--save exported-rider.json --rider alexa` (or another explicit source slot)
  to preview a normalized profile without modifying its original file. Restart
  restores the original in-memory preview, including consumed charms.

Both presentations use placeholder shapes. Headless Bevy ECS startup/input/pause tests
run without opening windows. Actual rendering and feel still need visual verification.
There is no microphone, transcription, live network connection or persistence in these
entry points.

## Implemented core

- Flight, floating-stick input, viewport bounds and timer ordering.
- All six breath patterns, four rider weapons, projectile motion/homing/hammer return,
  collision ordering, armor/mark/crack/mega damage and host-versus-guest hit authority.
- All 20 enemy definitions, spawn placement and movement/attack brains. World effects
  are explicit events; independent brain parity is not full-world parity.
- All 24 base mission definitions with the original stretch rules, journey/wave
  coordination, banking, result timing and guest branches.
- All eight objective definitions and protect/escort/carry geometry, damage and theft.
- Player bumps/bonks, hazard and lightning timing, gem movement/collection geometry.
- Integrated solo m1–m24 lifecycle, including all objective types and enemy effects,
  retirement and success/failure results. Normalized profile parties, wings, crafts,
  skills, charms, specials and spells are integrated. Completion mutates only the
  in-memory profile. Champion variants and milestone wild allies are integrated;
  live co-op is not yet integrated.
- All specials, eight spells and Daddio powers, with explicit ordered world effects.
- Roost hatch/release/milestones and source completion rewards, with injected time,
  day and RNG. The prototype uses synthetic day/time, not real daily persistence.
- Source times-table hatch gate and freeze/reload/countdown state with injected time,
  including the five-answer requirement and delayed wrong-answer deadline. UI/storage
  callbacks are intents, not live writes.
- Original story, briefing, quest and project text plus journal/shared unlock rules.
- Built-profile offline adventure flow: story/briefing to mission, pause/resume, result
  progression to story/rank/results, growth scenes and times-gate to hatch/roost. The
  current source allows Continue/skip immediately; narration and heard-page history
  remain host responsibilities. Builder/lobby/result prose and
  retry-from-stop UI remain separate. The initial supplied profile stays untouched.
- Pure profile normalization and in-memory progression commands (XP, feeding, treats,
  crafts, skill purchases, party selection, team craft/hoard aggregation).

`BrowserSave` keeps source JSON bytes and unknown fields untouched. Its normalization
method produces a separate value and supports ordinary legacy/current saves, Unicode
names and UTF-16 length rules. Unsupported pathological string coercions, inherited
prototype keys and surrogate-splitting truncations return errors rather than silently
rewriting data. The simpler flight projection is deliberately conservative. Storage
archives preserve every original key/value string, including freeze, display, narration,
history, invite, volume and future keys. None of these APIs access live storage.

A pure replacement plan can produce an edited archive and exact rollback bytes for
an explicit rider slot. It rejects stale originals and changed identities, retains
unrelated entries/root extensions and safe stable-ID dragon extensions, and replaces
canonical nested gameplay fields rather than resurrecting deleted values. This is
not a browser write adapter. A separate Unix adapter can publish an explicitly
selected archive into a NEW file with a verified backup, without overwriting the
source. It uses exclusive creation, exact-source rechecks and same-directory atomic
hard-link publication. Existing-file replacement and live-browser reload are not
implemented. Directory ACL privacy must be established by the host; mode bits alone
are not a universal privacy guarantee. Unsupported platforms/filesystems fail closed. Recovery checks the exact expected replacement before restoring backups.

## Networking and privacy

`dragon-peer` exposes a reusable Rust session API and an opt-in native Matchbox backend
using the existing Dragon namespace, channels, packet limits and version checks. Native
voice is disabled. The existing browser microphone/voice API remains opt-in; no
transcription is connected. See `crates/dragon-peer/README.md`.

A genuine two-native-client loopback test passes reliable/unreliable exchange, replay
rejection, disconnect and cancellation using ephemeral local signaling and no STUN/TURN
URLs. This is not a browser/native interoperability or multiplayer gameplay test.
Pure static-room authority election, world snapshot encoding/guest projection,
trusted-sender action admission and host takeover/handoff are ported separately. They keep transport identity distinct from rider claims, preserve
valid source wire layouts and quantization, and return presentation/banking effects.
Malformed entity IDs and indices are rejected more strictly than the JS receiver.
The `coop::Session` adapter now connects those rules to the mission runtime: host-owned
enemy/objective updates, guest hits, local rewards, Guardian healing, normalized resize,
pause-aware host timeout, disconnect takeover and explicit handoff. Seven in-memory
two-player tests cover these paths, including real `dragon-peer::Session` packet
admission and replay rejection before gameplay. The application must still bind verified
room slots, drive an actual socket, and render remote players. These tests do not yet
establish a live two-device Dragon mission or browser/native interoperability.

## Verification

`cargo test -p dragon-game -- --include-ignored` runs portable tests and, on macOS,
JavaScriptCore oracles that extract actual legacy functions. Coverage includes:

- 3,360 flight frames and viewport geometry;
- 1,152 breath and 64 weapon configurations;
- 108 projectile fixtures over four frames;
- 12,000 enemy frames with state, hazard and strike comparisons;
- 328 damage cases, 128 gem cases and 102 ability scenarios;
- 512 authority-election combinations, 24 hatch and 480 completion-reward cases;
- 48 profile/party shooting-and-wing configurations;
- all 24 base-mission lifecycle/objective regressions and source realm/effect checks;
- original story/content catalog and 3,072 mission-unlock cases;
- times/freeze interaction traces and 421 local-day/reload boundaries;
- mission definitions/lifecycle, objective geometry, profile normalization and
  progression mutations against synthetic source-JS fixtures.

The oracles explicitly stub presentation or unported external callbacks. They are
component evidence, not a claim that an entire live mission or old RNG session matches.
Native and WASM lint checks cover the prototypes; required CI must be checked on each
published head. No personal save was used in these tests.

## Timing decision and remaining replacement gates

The original variable-frame integrator is not equivalent to CrabScape's fixed 120 Hz
tick. Preserve explicit frame schedules and the original 50 ms cap using f64. A replay
must record inputs, frame times, viewport/HUD geometry, initial state and random draws.
The current Rust seed is not a replay of a browser Math.random session: cosmetic draws
are not all consumed. Do not claim fixed-tick or full-world identity from isolated tests.

Before replacing the JS game: complete live co-op integration,
full rewards/roost/story/egg UI and
multiplayer; reproduce presentation and controls; compare complete interrupted and
repeated play flows on phones and desktop; verify browser/native sessions; implement an
explicit reversible save transfer/write flow with failure tests. Keep original saves and
the live JS game available until those gates pass. The embedded home-server client wrapper uses WebSocket `/live` with get/set/sub
messages and `riders/<slot>` documents containing the same profile payload. The
server implementation, on-disk layout/export format and conflict handling remain
unverified; no home server or personal save has been accessed.

## Agreed world entry

The intended final entrance is a physical dragon on the castle's High Ramparts,
not a menu link or replacement of CrabScape. The existing roof is `royal::ROOF_Y`;
its central walkable span is x=1205..2910, clear of both access ropes. A landing
around x=2300 leaves the existing bell and other castle interactions untouched.
The player walks/climbs there, boards the dragon, and flies into Valmora. A return
flight brings the player back to the roof with the same CrabScape state. Dragon
progress is a separate state, not a conversion of the CrabScape save.

Implementation must arbitrate input with the existing Court/settings/inventory,
retain the CrabScape world and player state across the transition, and test entry,
cancellation, return, reload, and multiplayer presence. Do not activate the route
as a completed Valmora replacement while the core/presentation/save gates above
remain unfinished. The standalone JS game stays available.

### Rendering reference

The original procedural drawing functions remain the visual source of truth. Record
Canvas commands, transforms, gradient stops and animation inputs from the original
functions before translating them to native paths/meshes. Preserve quirks unless a
separate change is agreed: `shade` parses a hex string, but some drawing callers pass
its `rgb(...)` result back into `shade`; parsing that string follows the original JS
NaN/bitwise behavior. A conventional color helper would silently change those colors.
Body, wing, rider and enemy geometry should be compared independently before complete
scene images. Placeholder prototype rectangles are not evidence of visual parity.
