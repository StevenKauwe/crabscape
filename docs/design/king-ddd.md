# The kingdom above water

The owner asked for the existing King Data Driven Decision arena to become an absurd native CrabScape location
once its standalone browser version worked. That original remains at `/king-ddd/`.

Accept **Royal summons** in the game, or open `/?king-ddd`. The existing Home command safely rests the crab in its
shell, then the same separate-scene convention used by the shell interior presents a court at world y=40,000.
Returning restores the shell view. Visiting never changes the ocean's save format or invents air-breathing physics.
The court's workbook state is independent of the ocean save. Reloading restores the crab, not an unsaved workbook;
export XLSX to keep a workbook.

## Native implementation

- `src/king_ddd`: real GridCraft `SheetApp` through `bevy_egui`, Bevy court sprites, explicit input ownership,
  file import/export and background-worker bridge. The browser uses generated WASM glue, not an embedded page.
- `crates/king-ddd`: column roles, domains/presence, overlapping count/sum collections, lookup features, bounded
  arithmetic merits, real independent TabICL outcome models, probabilities, ranking, reaping, evolution and archive.
- `tools/kingworker`: the same Rust core in a dedicated Web Worker so model training does not block the game.
  The native PC build uses one background thread, cooperatively cancelled between model fits and generations.
- `vendor/gridcraft` and `vendor/tabicl-anywhere`: source snapshots reused from the actual original attempts.
  Their own licences and notices remain intact. CrabScape's code and art remain all rights reserved.
  GridCraft retains the host's `serde_json` parser settings: its standalone `float_roundtrip` feature changes
  f32 overflow decoding globally and would break recovery of a damaged crab's progress without losing terrain.
- `vendor/bevy_egui`: version 0.43.1 with its unused `zstd_rust` image feature removed, avoiding the vulnerable
  decoder in RUSTSEC-2026-0344. Egui's generated textures and the court's PNG/JPEG images do not use it.
  GridCraft's XML parser is upgraded to quick-xml 0.41 for RUSTSEC-2026-0194/0195. Font notices are distributed
  with the court assets; dependency auditing stays mandatory.
- `assets/king-ddd`: the original launch workbook and oil-painted ShapeStage knights/battlefield. The court's
  resident knights retain their armour; each new selected candidate challenges in the opposite armour. Actual
  candidate scores decide who survives. The king's preliminary puppet is made from individual Bevy shapes.

The browser downloads the existing pretrained model weights lazily from `/king-ddd/vendor/engine/weights/`.
No table, candidate or model data is sent to a server. Only needed model kinds are loaded. Generated worker modules
are validated without relaxed SIMD, and the game retains its WebGL2 path.

## Scope

The native court has an editable XLSX/CSV workbook, role and goal editing, numeric intervals/categories/presence,
lookup columns, safe merit formulas with historical-value writeback, overlapping collections, tournament
configuration, actual model training, rank-based reaping, survivor details and winner writeback/export. Numeric
class labels can explicitly be classified instead of regressed. Winner sheets retain categorical text and IDs,
predicted medians/intervals or classes, and merits. The sample has its original lookup and group,
plus a ready-to-run customer goal. Changing the viewed workbook sheet does not silently change the training sheet.

The original optional external LLM assistant and persistent fitted-model cache are not part of this edition.
Workbook exports preserve data and sheets; arena configuration currently stays in memory for the visit/session.
This is a separate court scene, not a freely traversable land biome.

## Focused verification

- `cargo test -p king-ddd`: constraint/evolution, bounded formula and real classifier tournament regressions.
- `cargo test -p crabscape --bin crabscape king_ddd`: sample/preset, sheet viewing, lossless winner writeback,
  merit history, royal font initialization, real animation manifest and court camera tests.
- `cargo run -p king-ddd --example royal_trial -- <weights-directory>`: actual three-model, two-generation
  acceptance run using the original pretrained regression weights.
- `tools/build-web.sh`: native Rust worker build and no-relaxed-SIMD validation.
- `cargo run --profile art -p cooptest --bin king -- [--url <site>] [--train]`: isolated headless Chrome smoke,
  spreadsheet typing versus ocean shortcuts, phone layout, return and reload. `--train` also performs the actual
  8-candidate, two-generation tournament and writes the winners into a new workbook sheet. It uses its own
  profile and grants no clipboard permission.
