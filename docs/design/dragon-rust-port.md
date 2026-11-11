# Dragon Rust port: flight slice

This branch is an additive, incomplete port, not a replacement for Dragon. The existing
`web/dragon/index.html`, browser save keys, public build and Dragon peer/voice transport
are unchanged. Do not route players to this sandbox as the full game.

## Run and verify

- `cargo run --bin dragon_flight`: native Bevy flight sandbox; arrows/WASD to steer,
  Escape to pause, R to reset. It uses placeholder art. No browser or GUI was opened
  during implementation; rendering still needs a human-visible check.
- `cargo run --bin dragon_flight -- --save exported-rider.json`: read an explicitly
  supplied v3 rider export to preview bonded dragon size, solo tea and magic bonuses.
  No personal save discovery or write occurs. An invalid/unsupported file stops import
  rather than replacing it with a new rider.
- `cargo test -p dragon-game`: save preservation and numerical flight regressions.
- `cargo test -p dragon-game --test legacy_flight -- --ignored`: on macOS,
  compare 3,360 synthetic frames against functions extracted from the real JS source
  using JavaScriptCore. Combat effects are stubbed to events and cosmetic RNG disabled.
- `cargo check --bin dragon_flight`: compile the Bevy presentation.

## What is ported

`crates/dragon-game` is a presentation-independent Rust core. It takes explicit input,
frame duration, screen geometry and flight rules. It ports the flight portion of
`updatePlayer`, including timer ordering, energy recovery, retired/resting/dizzy paths,
shadow speed, facing, floating-stick dead zone, viewport scaling and bounds. Combat
callbacks are **events only**; they do not yet perform combat or consume randomness.
Shadow particles, wing animation and other cosmetic randomness are not simulated.

The Bevy app converts input into that core and presents its position. Placeholder art
is deliberately not a claim of visual parity. The sandbox does not exercise all the
core states through gameplay, because the mission/combat systems are not ported yet.

The compatibility adapter retains original JSON byte-for-byte, including unknown fields.
It exposes a read-only projection rather than claiming to implement the entire browser
normalizer. In particular, projection accepts canonical JSON numbers rather than
JavaScript numeric-string/boolean coercions, and requires a boolean bonded flag.
Old XP conversion applies only to the projection. A storage archive retains
all original key/value strings, including auxiliary freeze, display, narration, history,
invite, volume and future keys. The adapter does not access localStorage itself, import
into live storage, synchronize remote rider documents, or write upgraded saves. Browser
progression saves do not contain a full running mission or RNG state.

## Timing decision

A fixed 120 Hz tick is not equivalent to the original game's variable-frame integrator.
For example, one 1/30-second acceleration step differs from two 1/60-second steps.
This slice preserves explicit frame schedules and the original 50 ms cap, using f64
like JS numbers. Before replacing the live game, record and compare frame schedules,
inputs, viewport/HUD geometry, initial mission state and random draws. Do not silently
subdivide these steps into CrabScape ticks and call the result identical.

## Remaining work before replacement

1. Port mission definitions, enemy AI, attacks, collisions, objectives, rewards, progression,
   roost/crafting/story flows and the complete profile normalization rules.
2. Capture/inject gameplay RNG separately from cosmetic draws and compare full missions
   against the source. Cover phone layouts and interrupted/repeated interactions.
3. Reuse `crates/dragon-peer` and CrabScape's Matchbox connection infrastructure for
   actual sessions, authority/handoff and shared world state. The existing Dragon peer
   transport is browser/WASM-only. This sandbox has **no multiplayer**; native transport
   needs explicit implementation and two-client tests, not an unrelated new protocol.
4. Port visual/audio presentation and verify it without changing the live JS deployment.
5. Add a staged, explicit save transfer/write flow with backups, failure tests and a
   proven return path to the browser. Keep current save keys and original data intact.

## Privacy boundary

This slice does not connect, capture audio, enable a microphone or invoke transcription.
The existing Dragon peer voice path remains unchanged. Future multiplayer wiring must
keep microphone capture off by default and never attach the `listen` transcription
pipeline to Dragon voice. Verify reconnect and disconnect behavior before rollout.
