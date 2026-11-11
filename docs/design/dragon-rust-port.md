# Dragon Rust port: work in progress

This branch is an additive port, not a replacement for Dragon. The existing
`web/dragon/index.html`, browser save keys and public game remain unchanged. The PR's
standalone Dragon preview remains the existing JS game. The CrabScape preview now has an explicitly labeled native-port castle trip.

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
- `cargo run --bin dragon_adventure -- exported-rider.json rider3`: native adventure
  presentation for an explicitly supplied built profile. Story, unlocked mission map,
  roost feeding/crafting/skills, egg questions/hatching, results, pause and touch controls
  use the ported adventure state machine. It never writes the input file. This explicit-file harness remains in-memory. The main CrabScape route uses separate native checkpoints.
- With no arguments, `dragon_adventure` starts the two-step new-rider builder in an
  isolated, in-memory rider3 profile. Existing riders can change their look at the
  roost without resetting progression, identity, original import or adventure RNG.

The flight/mission harnesses use placeholder shapes. The adventure uses original
procedural dragon/rider and enemy geometry, attack warnings, strikes, hazards and
objective glyphs, shots/gems/allies and the six source backdrops/scenery, tessellated
into egui meshes. Source audio, browser narration, data-only lobby, builder/roost
previews, weather/trails and foregrounds are integrated. Other scene previews,
celebration effects and complete screen layouts remain unfinished. Canvas antialiasing,
stroke joins and anisotropic strokes are approximations; command equality is not
pixel equality. One viewport/realm/song backdrop is cached and dynamic outer buffers
are reused, but nested paths/paints and dynamic meshes still allocate per frame. No
phone performance improvement is claimed or measured. Headless Bevy ECS startup/input/pause tests
run without opening windows. Isolated Chromium journeys verify actual rendering,
checkpoint reload and synthetic two-thumb controls; physical-phone feel and WebKit
remain separate verification gates.
The adventure now offers an explicit data-only room connection. Boarding the castle dragon turns off
CrabScape microphone capture and transcription, drops pending transcription results,
and rejects late microphone permission callbacks. Returning does not turn the mic on.

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
  skills, charms, specials and spells are integrated. Completion mutates the
  profile, with separate checkpoints in the castle route. Champion variants and milestone wild allies are integrated;
  live co-op uses the same owned Game through the shared Session adapter.
- All specials, eight spells and Daddio powers, with explicit ordered world effects.
- Roost hatch/release/milestones and source completion rewards, with injected time,
  day and RNG. The mission harness uses synthetic day/time. Adventure obtains the
  system's local-day offset (including DST); the castle route checkpoints the complete profile.
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
- 268 original Canvas command streams covering dragons/riders, every enemy kind,
  squash/attack warnings, hazards/strikes, objectives, shots/gems and backgrounds/scenery;
- 32 source builder-default/RNG-order comparisons, plus edit identity and progression checks;
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

Before replacing the JS game: complete party readiness/countdown, cross-browser/native co-op validation,
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

## Castle preview checkpoint lifecycle

The roof dragon at x=2300 admits a grounded nearby player using V or the boarding
button. A cancellable flight opens the native rider menu, and return keeps the
existing CrabScape world/player rather than replacing its save or teleporting it.
Input ownership blocks underlying shortcuts and clears stale held inputs on return.
This is a preview route, not an announcement of full behavioral or visual parity.

Native progress uses a private `.crabscape-valmora-v1` directory (gitignored); WASM
uses an isolated IndexedDB namespace, including per-PR/branch preview isolation.
No legacy browser keys are discovered or overwritten. Selecting a slot loads its
checkpoint or offers the source two-step builder. Explicit pasted profile/archive
imports retain original bytes and unknown fields; explicit copy buttons provide
current profile, saved compatible export and original archive backup separately.
Native durable writes currently support Unix and fail closed elsewhere.

A bounded asynchronous driver admits one operation, retains the last successful
revision token until commit confirmation, and blocks writes after uncertain/stale
failures until an explicit reload. Dirty/flush events and a two-second comparison
checkpoint live progression without per-frame disk/JSON writes. Freeze state is
atomic with the profile; admission scans all six slots for a still-active freeze.
An error keeps unsaved progress in memory and exposes recovery export before the
explicit discard/reload action. Checkpoints do not resume a live mission or RNG.

Synthetic tests cover actual builder completion, native torn writes/concurrency,
IndexedDB transaction races, quota failure, malformed preservation and freeze expiry.
Actual isolated Chromium ran the storage adapter; full root journey screenshots,
phone controls/performance and browser/native live co-op remain validation gates.
The home-server disk format remains unverified.

## Live-room increment

The native adventure can opt into a code-named data-only Matchbox room before a
mission. Native and WASM use CrabScape's introducer and the Dragon channel protocol,
with an exact-build packet guard and a separate commit-prefixed room namespace.
They do not silently join legacy JS rooms or an existing CrabScape room. Browser
voice signaling/playback/microphone are disabled for this connection.

The adapter admits transport peers, rejects conflicting rider slots, discovers an
existing mission host/run, and wraps the already-launched Game in coop::Session.
It preserves local profile ownership, keeps paused presence while visiting the castle,
flushes final packets before mission retirement, and provides explicit leave-to-roost.
Partner lead-dragon/rider rendering and normalized position interpolation are wired;
complete follower/trail/presentation-effect parity remains unfinished.

An explicit localhost-only test establishes two real native WebRTC clients, joins a
shared Adventure, checks host/guest/run/profile ownership, and verifies takeover after
host disconnect. It uses no public signaling, STUN, GUI, audio or real saves. The
fixture waits for first server admission before joining: upstream full_mesh separates
its room snapshot and insertion locks, and simultaneous first arrivals were observed
with IDs but no peer connection. This does not establish the production server's
implementation or prove browser/native interoperability.

Party now follows the source newest-pick, Ready, three-second countdown and final
half-second commitment rules, including lowest-ready-slot host election. The actual
two-client test drives Party through shared launch and takeover. A direct source-JS
oracle compares 500 countdown frames, including interrupted readiness. The UI also
supports explicit start/join without waiting for every connected rider.

An isolated headless root build has rendered the physical rooftop dragon, entered
Valmora, selected a slot, traversed both builder steps, created a durable checkpoint,
returned to the castle, and reloaded the same saved opening story without requesting
a microphone. Read-only `data-valmora` stage assertions verify the actual transitions.
Mission gameplay/rendering and full co-op journey validation are still in progress.
The dark source palette now covers the root world/HUD while in Valmora menus.
Imported unbuilt profiles keep their rider identity when completing the builder.
Browser/native interoperability, mobile controls/performance, remaining presentation,
and full audio/narration remain open gates.

## Audio, device preferences and presentation checkpoint

The runtime now consumes a bounded presentation-only sound queue for combat,
abilities, pickups, mission transitions and validated guest snapshots. Adventure
retirement retains final-frame cues. Original-JavaScript comparisons cover sound
recipes, music scheduling, narration segmentation, hit/defeat and guest world cues.
Browser playback schedules WebAudio nodes directly. Native music renders on a
bounded worker; late batches are dropped instead of shifting the beat. Native SFX
cache finite variants. Neither backend accesses a microphone or recording API.
Browser narration uses source voice scoring, sentence/quotation parameters and
word-boundary highlighting; Continue still skips immediately. Native speech and
recorded-clip playback are not implemented. The browser clip adapter exists, but
home-server clip discovery is not integrated or verified. Audible/live narration
QA is still required; PCM spectra are not claimed bit-identical to WebAudio.

Device-global preferences have their own exact revision token and failure latch,
separate from rider checkpoints: music, narration, view size, fancy graphics and
last-300 insertion-ordered heard pages. Explicit archive imports apply only known
provided keys; missing keys and profile-only imports leave native preferences
alone. Original archive bytes remain immutable. Actual Chromium tests cover
preference concurrency, stale tokens, quota failures and malformed-record retention.
Master mute remains ephemeral. Runtime view scaling covers gameplay and rendering.

Fancy mode adds original moon rays, six realm foregrounds and bounded projectile
lights. The source command oracle includes 22 new atmosphere/light cases. Partner
followers retain bounded presentation positions and source smoothing. Ambient
particles and trails remained unfinished at this checkpoint; builder/roost previews
are covered by the subsequent checkpoint below. Vignette/full-frame
flash overlays are omitted under the repository's overlay restriction.

The host-only browser/native WebRTC fixture exchanges reliable messages in both
directions with zero microphone requests. Native dependencies now use maintained
Tokio/rustls signaling and DTLS, and both cargo-deny graphs pass without suppressions.
This is transport validation, not a claim that the full two-player UI journey or
phone two-thumb controls have completed QA. The port remains a draft, unmerged.

## Preview and two-thumb validation

Builder and roost previews now use the source standing rider, egg, hatchling/adult
and miniature dragon drawing. Direct live-JavaScript oracles pass 90 builder
combinations and 120 follower update frames, including facing changes and varied dt.
Other scene previews remain open; rasterization is not pixel-identical.

Touch controls now track distinct device/finger IDs independently from egui's mouse
pointer: steering and held fire can coexist, a new sky touch replaces a stale stick,
and special/swap/spell/dragon buttons act on touch-down while the other thumb steers.
Cancellation, focus loss and castle return clear captures. Four regression tests
cover these rules. An isolated Chromium root journey exercised both builder previews,
checkpoint reload, story, party and First Flight; two concurrent CDP touch points
steered the dragon and fired, and releasing removed the stick. Pause, view and fancy
controls were inspected in the actual runtime. No microphone requests or runtime
errors occurred after the repaired input borrow boundary. This is desktop Chromium
synthetic-touch validation, not physical iPhone or WebKit verification. Non-flight
adventure screens scroll so long mission/roost lists remain reachable.

The dedicated `dragon_audio_probe` and `cooptest` `dragon_audio` runner verify real
WebAudio node scheduling, gain buses, caps, cancellation and cleanup. The runner
self-checks that failing assertions are rejected. Synthetic speech-service callbacks
verify UTF-16 ranges, completion/error and stale-callback isolation after timeout.
Chrome returned zero available voices: actual speech rendering/listening remains
unverified. Native narration controls explicitly show unsupported availability.

## Browser import, weather and full co-op fixture

The selected empty native slot now offers an explicit same-browser original-save
import. It reads only that rider key and eight known source globals, never enumerates
storage or writes/deletes a legacy key. Exact raw strings are retained, including
voice-volume/lobby bytes that are never applied to voice or transport. Source changes,
occupied destinations, denied reads, malformed input, lossy UTF-16 conversion and
quota failures fail closed. Synthetic Chromium verifies all these cases and rollback;
no personal saves were inspected. Exported JSON import remains available on native.

Six-realm weather and Daddio trails use a private visual RNG, with one update per
visual frame rather than per egui paint pass. The recorded-JS oracle covers 1,440
frames, source RNG consumption and the source's nonfinite twinkle-position behavior.
Chromium confirms nonfinite Canvas translations leave the transform unchanged.
Weather/firefly caps match the source; trails have an explicit 512-particle safety cap
instead of the source's unbounded array. Browser reduced-motion suppresses trails.
Full-frame flash/vignette effects remain deliberately omitted.

The `valmora-loopback-test` feature and `cooptest` `valmora` runner exercise two real
browsers through the actual rooftop trip, builder, story, room and Ready controls.
The fixture uses a localhost-only introducer with host-only ICE, distinct synthetic
profiles and read-only geometry/state telemetry. It verifies distinct riders, one
host, matching mission/run, guest movement, and takeover after the host leaves without
resetting the mission/run. Microphone and nonlocal signaling guards remain zero.
No gameplay state is injected to bypass the journey. This is Chromium validation,
not physical iPhone/WebKit gameplay verification or cross-version protocol support.


The phone flight HUD now follows the source layout roles: compact top status/Pause,
team bottom-left and fire/abilities bottom-right, leaving the sky available for the
floating stick. A 390×844 Chromium journey imports a synthetic legacy rider, preserves
its exact source bytes and auxiliary flags, enters First Flight, steers/fires with two
simultaneous touches, then pauses with the second thumb while the first remains down.
Releasing does not resume or leave stale captures. Layout tests cover 320×568,
390×844 and 844×390; cached closed-story state cannot hide the flight HUD. Invalid
imports expose an explicit draft-discard/slot-reload recovery action.

## Live team progression, Roost actions and scene fidelity

Joined native rooms now use the source's non-STATIC team math; solo remains STATIC.
This is an explicit native-room rule, not a claim that the original standalone P2P
page shared its economy. Bounded, validated live projections supply current hoard,
crafts, project unlocks, combat bonuses and reward multipliers. Transport-bound rider
identities, six-second freshness and away/disconnect filtering determine membership.
No original archive, save/dragon IDs, settings or local save file is transmitted or
merged. Castle return advertises away and removes flight poses instead of pretending
the rider remains paused in the sky. Combat reads scalar team bonuses without
allocating a JSON summary per projectile or hit.

Build and Craft use reliable serialized room proposals. The majority/tie rules match
364 direct original-JavaScript combinations; a real two-native-client WebRTC test
verifies proposal, Yes, one spender, updated accounting, shared flight and takeover.
Native safety differs from the home server's non-atomic claim/write flow: an ungranted
vote is cancelled on membership changes; ambiguous grants, coordinator changes and
castle departure block further purchases until reconnect. There is no departed-
proposer fallback that spends another rider's save, and no retry of an ambiguous
purchase. Sessions cap rounds at 128 to retain bounded replay protection. Personal
feed/charms remain personal source actions; this is not a persistent shared ledger.

The Roost exposes source project construction, daily challenge, charm purchases,
spellbook loadout, voluntary Valley release and journal replay. Source action tests
cover limits, costs, locked content, bonded-dragon protection and retained selected
words. An additional live JavaScript oracle compares 88 charm/loadout state
transitions. Dynamic growth/lesson pages no longer disappear when a static story
catalog lookup has no corresponding page. Per-screen scroll identity prevents a
long Roost scroll from hiding a newly opened story.

Original story/growth, hatch, Valley, riddle egg, frozen dragon and milestone art
passes 53 live-source geometry cases. Healing uses its complete original static
project/workshop painting and animated layer, checked against 44 source cases.
Its bounded cache includes size, DPI, project set and workshop inputs; repaint-only
random colors use a private presentation RNG. Cubic curves, rounded rectangles and
animated dash offsets have bounded tessellation tests. The source's full-canvas haze
and vignette are deliberately omitted under repository art rules. Rasterization,
font outlines and physical-device display still are not pixel-identical guarantees.

A synthetic 390×844 Chromium journey imports the selected legacy rider through the
actual UI, views Healing/Valley, buys a charm (1000→975 gems), changes spell loadout,
replays a journal page and builds the bell tower (975→895), updating the painting.
The original browser save remains unchanged, with zero mic requests/runtime errors.
This does not verify physical iPhone behavior, native speech, actual recorded voice
clips or the unavailable home-server save wrapper/conflict format.

The final two-browser UI fixture also passes explicit original-save import, actual
armor proposal/Yes/grant, exactly one spender and both riders' team armor, then
Ready launch, guest movement and higher-epoch host-loss takeover without resetting
the run. Legacy bytes, microphone and nonlocal-signaling guards remain unchanged.
Completed votes hide after seven seconds and all vote banners hide during flight.

The interaction follow-up restores source checkpoint retry, pause exit and keyboard
behavior. Failed non-Champion runs can restart at the reached stop with fresh mission
state and retained banked progression. A retry deliberately creates a fresh host run
instead of rejoining a stale same-mission room advertisement. Pause exit emits the
old-run handoff without banking carried gems. Tests cover every normal mission stop,
rejected retry states, failed-connection retry intent and ordinary launch rejoining.

Riddle Backspace now clears rather than appending its key name; numpad entry and
source pause/resume/fire shortcuts are covered. Pause clears held controls without
seeding stale keys on resume. Daddio has both source power buttons on touch layouts.
Full-party hatches can send the new baby to the Valley, retain an intentionally blank
name while editing and submit with Enter. Winning results expose egg waking directly.
An actual synthetic 390×844 Chromium journey verifies cleared riddle input, five
numpad answers, the full-party baby option and Enter completion, with no microphone
requests/runtime errors and unchanged original legacy data. These checks do not
remove the physical-device, recorded-audio or home-server compatibility limits above.
