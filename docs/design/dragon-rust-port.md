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
  m1 through m4 currently supported; Space fires, WASD/arrows move, Escape pauses,
  R restarts. All use a deterministic test seed and a starting bonded dragon.

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
- Integrated solo m1–m4 lifecycle, including the barn objective, Thunderhead phases,
  rewards within the runtime, retirement and success/failure results. Full persistent
  progression, party/rider actions and the remaining missions are not yet integrated.
- All specials, eight spells and Daddio powers, with explicit ordered world effects.
- Pure profile normalization and in-memory progression commands (XP, feeding, treats,
  crafts, skill purchases, party selection, team craft/hoard aggregation).

`BrowserSave` keeps source JSON bytes and unknown fields untouched. Its normalization
method produces a separate value and supports ordinary legacy/current saves, Unicode
names and UTF-16 length rules. Unsupported pathological string coercions, inherited
prototype keys and surrogate-splitting truncations return errors rather than silently
rewriting data. The simpler flight projection is deliberately conservative. Storage
archives preserve every original key/value string, including freeze, display, narration,
history, invite, volume and future keys. None of these APIs access live storage.

## Networking and privacy

`dragon-peer` exposes a reusable Rust session API and an opt-in native Matchbox backend
using the existing Dragon namespace, channels, packet limits and version checks. Native
voice is disabled. The existing browser microphone/voice API remains opt-in; no
transcription is connected. See `crates/dragon-peer/README.md`.

A genuine two-native-client loopback test passes reliable/unreliable exchange, replay
rejection, disconnect and cancellation using ephemeral local signaling and no STUN/TURN
URLs. This is not a browser/native interoperability or multiplayer gameplay test. The
mission runtime still needs world/session synchronization and authority/handoff wiring.

## Verification

`cargo test -p dragon-game -- --include-ignored` runs portable tests and, on macOS,
JavaScriptCore oracles that extract actual legacy functions. Coverage includes:

- 3,360 flight frames and viewport geometry;
- 1,152 breath and 64 weapon configurations;
- 108 projectile fixtures over four frames;
- 12,000 enemy frames with state, hazard and strike comparisons;
- 328 damage cases, 128 gem cases and 102 ability scenarios;
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

Before replacing the JS game: integrate profile/party/skills/spells, every mission and
objective, synchronous defeat/split callbacks, full rewards/roost/story/egg flows and
multiplayer; reproduce presentation and controls; compare complete interrupted and
repeated play flows on phones and desktop; verify browser/native sessions; implement an
explicit reversible save transfer/write flow with failure tests. Keep original saves and
the live JS game available until those gates pass. The separate home-server wrapper and
its storage format have not been inspected or claimed compatible.
