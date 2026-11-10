# The kingdom above water

The owner asked for King Data Driven Decision to become an absurd, native CrabScape location after its
standalone browser version worked. That original remains at `/king-ddd/`, unchanged by the native edition.

Walk from home to the starting kelp, jump, tap Jump again to grip, and hold it to climb. Its upper fronds are
lashed to a rope at the surface. Keep climbing, then jump right onto the castle landing. The same player,
camera, crab puppet, walking, collision and exploration memory continue through the entire route. Walk off
the arrival side to return to the sea. There is no summons, teleport, separate scene or embedded browser.
`/?king-ddd` and native `--king-ddd` offer directions, without moving the crab.

This above-water kingdom is the owner's explicit exception to the otherwise ocean-shaped setting. Air has
gravity and no swimming strokes. A keep wall bounds the opening region so it does not bypass ocean gates.
The immutable masonry and ropes are ordinary deterministic build geometry, not editable sediment.

## The glowing ledger and the lists

A luminous workbook floats beside the landing. Stand next to it and press U or its contextual touch button
to read it. Only this ledger is a modal editor: the world stays at the real crab's position, and ordinary
HUD/input return when the book closes. Escape closes the ledger unless the grid is first cancelling an edit.
Starting a tournament puts the book down. Walk right to watch the lists; return to the book for results,
winner writeback or export. Closing the book does not cancel model fitting.

The king, clerk and tournament fighters use CrabScape's actual articulated crab rig and painted body parts.
The king wears a crown; candidates use the existing claw attacks, guarding and defeated poses. Real scored
candidates from completed generations enter sequential judged bouts. Their actual hit scores decide which
crab remains. This is a visual enactment of the optimizer, not a second combat-based scoring algorithm.

The workbook and tournament are device-local. The traversable region belongs to the ordinary world, but
other co-op players do not yet share this ledger or its staged tournament. Reloading preserves the crab's
castle position and exploration; export XLSX to keep an unsaved workbook. Existing memory pages and clock
survive the appended above-water extent. No ocean-save format is changed.

## Native implementation

- `crates/sim/src/level/royal.rs`: immutable castle surfaces, ropes and the bounded air region; deterministic
  physics handles their collision and the water-to-air transition. Existing underwater controls are retained.
- `src/king_ddd`: real GridCraft `SheetApp` through `bevy_egui`, an in-world book, castle masonry, native crab
  actors, explicit modal input ownership, file import/export and the background-worker bridge.
- `crates/king-ddd`: column roles, domains/presence, overlapping count/sum collections, lookup features,
  bounded arithmetic merits, independent TabICL outcome models, probabilities, ranking, reaping and evolution.
- `tools/kingworker`: the same Rust core in a Web Worker so fitting does not block the game. Native PC uses a
  background thread, cooperatively cancelled between model fits and generations.
- `vendor/gridcraft` and `vendor/tabicl-anywhere`: source snapshots reused from the original attempts with
  their licences intact. GridCraft retains the host's `serde_json` parser settings; `float_roundtrip` changes
  f32 overflow decoding globally and would break recovery of damaged crab progress without losing terrain.
- `vendor/bevy_egui`: 0.43.1 without its unused `zstd_rust` image feature, avoiding RUSTSEC-2026-0344. GridCraft
  uses quick-xml 0.41 for RUSTSEC-2026-0194/0195. Font notices ship with the sample workbook assets.

The browser loads pretrained model weights lazily from `/king-ddd/vendor/engine/weights/`. No table,
candidate or model data is sent to a server. Worker modules retain the no-relaxed-SIMD/WebGL2 path.
The original knight/battlefield images are not bundled into this native region.

## Workbook scope

The ledger supports editable XLSX/CSV, role and goal editing, numeric intervals/categories/presence, lookup
columns, safe merit formulas with historical-value writeback, overlapping collections, tournament settings,
actual model fitting, rank-based reaping, survivor details and winner writeback/export. Numeric labels may
explicitly be classified. Winner sheets retain categorical text, IDs, predictions and merits. The sample has
its lookup/group and a runnable customer goal. Viewing another sheet does not change the training sheet.
The optional external LLM assistant and persistent fitted-model cache are not included. Configuration stays
in memory for the session; workbook exports preserve data and sheets.

## Focused verification

- `cargo test -p crabsim --test royal_region --test surface`: continuous ordinary-control kelp/rope/landing
  traversal, cellular rope connection, return to the sea, air gravity and the remaining ocean boundary.
- `cargo test -p crabsim --lib pre_castle_memory`: existing explored pages and clock survive new unseen rows.
- `cargo test -p crabscape --bin crabscape king_ddd`: ledger proximity, sample/preset, sheet viewing, lossless
  winner writeback, merit history and font initialization.
- `cargo run -p king-ddd --example royal_trial -- <weights-directory>`: real three-model, two-generation trial.
- `tools/build-web.sh`: Rust game/worker builds and no-relaxed-SIMD validation when wasm-tools is installed.
- `cargo run --profile art -p cooptest --bin king -- [--url <site>] [--train]`: isolated headless Chrome walks
  from spawn, climbs the actual kelp and rope, lands at the ledger, types into the real workbook, checks phone
  layout, preserves a castle-position save through reload and returns to the sea. `--train` also runs the
  three-model 16-candidate tournament, watches native crab bouts and writes the survivors into a new sheet.
  The runner has its own profile and grants no clipboard or other browser permission.
