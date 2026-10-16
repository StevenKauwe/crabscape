# CrabScape: notes for Claude

A cozy, brutal, surreal, biologically true side-scrolling game: you are a heavy, lantern-carrying crab on the deep-sea
floor. Rust + Bevy. "Feels like Valheim. Plays like Silksong. Grows like RuneScape." See `docs/design/vision.md`.

The team is a few friends with small token budgets. Humans play, give taste and look at art; Claude designs, builds
and runs all process. Nobody reads code, so tests and CI are the safety net.

## Anchors (read the ones your task touches)
- Style and art direction: `docs/design/art-direction.md` (crab anatomy: `art/crab/anatomy-notes.md`)
- Tone, pillars, loop: `docs/design/vision.md`
- Ocean-shaped rules (biology, no air, real mechanisms only): `docs/design/ocean-shaped.md`
- How it plays (movement, combat, controls): `docs/design/how-it-plays.md`
- World, zones, economy: `docs/design/world.md`
- Current decisions: `docs/design/decisions.md`

## Rules that never bend
- **No licence.** All rights reserved. Never add a LICENSE file or licence fields.
- **Ocean-shaped.** Everything is biologically real; self-consistency never bends. Crabs breathe water: no air, air
  tanks, goggles or pressure helmets. Facts need real, opened sources; otherwise mark them "not verified".
- **Art varies inside each shape; never full-frame filters or overlays.**
- **Never commit generated junk:** `target/`, `web/pkg/`, `web/assets/`, renders, scratch, throwaway art, videos.
- Changing any of these is a direction call for the owner (StevenKauwe).

## Engineering principles (non-negotiable)
- **100% Rust:** game, tools and art pipeline. No JS, Python or Node in the pipeline.
- **Two targets:** WebAssembly for the browser, native for PC.
- **Runs on phones and desktop.** iPhone/WebKit: WebGL2 only, **no relaxed-SIMD ever**, keep `RUSTFLAGS` empty.
- **Multiplayer-ready:** input becomes commands (`MoveRequest`), nothing moves the crab directly; the sim is
  deterministic; co-op will be host-authoritative.
- **Headless deterministic sim:** game rules live in `crates/sim` (fixed 1/120 s ticks, no clocks, randomness only
  from a seeded generator stored in `Sim`), never in Bevy systems. The game only presents it. `cargo crabsim` runs
  it at any speed; replays reproduce bug reports exactly.
- **Tests prove promises:** `crates/sim/tests/scenarios.rs` for feel (jump peaks, the trench, coyote time, dodge...);
  `crates/crabrig/tests/rig.rs` fails if the rig SVG or parts file drifts from the puppet. Changing a feel number on
  purpose? Update the test and say so in the PR. Feel numbers live in `crates/sim/src/tuning.rs`.
- **Performance and download size matter.** Bevy runs with few features and a size-tuned release build; measure
  before adding features or dependencies.

## How we work
- **Humans bring taste; Claude does all process.** Merge your own PR when checks pass. Never ask a human to review
  code, approve anything, resolve conflicts or do git chores.
- **Ask humans only** for taste/direction calls (how it feels or looks) or outward-facing ones (going public, money,
  contacting people): briefly, in plain words, with a recommendation.
- **Talking to humans:** plain words, no jargon (say "your change", "the automatic checks", "it's in the game now").
  Anything a human must do comes as one of: a yes/no or one `! command` in chat, a button on an artifact page (Studio:
  https://claude.ai/artifact/PttxVTZCC75dqSEpKXYowf), or one link that lands on the action. Never "go to Settings…".
- **Taste input** comes from the Studio page and `feedback` issues; `/triage` turns it into tasks.
- **Small PRs, one issue each.** Work in your own git worktree when other sessions may be active.
- **Respect the token budget:** pick tasks that fit, stop at a clean point, leave a handoff comment if unfinished.
- **Progress over perfection.** Nothing is precious; anything can be changed later by anyone's Claude.
- New or changed decisions go in `docs/design/decisions.md`.

## Repo practices and CI/CD
- Branch (`issue-<n>-<slug>`) → PR (`Closes #<n>`, what changed, how verified, screenshots for visual changes) →
  `gh pr merge --squash --auto --delete-branch`. Rebase on `main` and resolve conflicts yourself. Never push to `main`.
- CI (`.github/workflows/ci.yml`): fmt, clippy (native + wasm32), tests, the tidy tool's checks, the web build,
  relaxed-SIMD check, a size report. On `main` it deploys `web/` to GitHub Pages
  (https://stevenkauwe.github.io/crabscape/). Branch protection requires CI only.
- Issues: `claude-ready`, `size:S` (~30 min of Claude work), `size:M` (~1 h); `size:L` must be split; `needs-human`
  means a person must decide or draw first (don't start it). Areas: `area:code|art|research|design|tooling`.
- Skills: `/next-task` (claim, do and merge one issue), `/review-pr <n>` (optional self-check), `/triage`
  (feedback → tasks), `/doctor` (fix a broken computer or project copy).

## Setup and commands
Setup: Rust from https://rustup.rs, then `rustup target add wasm32-unknown-unknown`,
`cargo install wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>`, optionally `cargo install wasm-tools`
and binaryen's `wasm-opt`. Linux also needs `libudev-dev libwayland-dev libxkbcommon-dev pkg-config`.

| Job | Command |
|---|---|
| Run the game (native) | `cargo run` |
| Format | `cargo fmt` |
| Lint (native) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Lint (wasm32) | `cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings` |
| Test | `cargo test --workspace` |
| Web build into `web/` | `tools/build-web.sh` (serve with any static server, e.g. `python3 -m http.server -d web`) |
| WebKit (iPhone engine) test | `python3 tools/test-webkit.py [--landscape] [--shots /tmp/shot]` (needs WebKitGTK 4.1) |
| Headless sim | `cargo crabsim --script "t=0 walk=right; t=1 jump; t=1.45 jump" --seconds 4` |
| Replay a bug report | `cargo crabsim --inputs replay.txt --dump -` (F9 in the game saves a replay; `?replay` adds a button on the web) |
| Crab rig → SVG | `cargo run -p crabrig --profile art -- parts` (writes `art/crab/crab-parts.svg`) |
| Rig → game atlas | `cargo run -p crabrig --profile art -- export` (writes `assets/crab/`) |
| Oil-paint the atlas | `tools/paint-oil-crab.sh` |
| Oil painter | `cargo run -p painter --profile art --bin crabpaint -- oilphoto in.png out.png` |
| Disk housekeeping | `cargo tidy` (`cargo tidy report` only looks) |

Feel change? Run the same crabsim script on `main` and on your branch and compare the numbers (`cargo crabsim --help`
lists the script format and flags). Visual change? Attach before/after screenshots.

Layout: `crates/sim` rules · `src/` the Bevy game (drawing, input, camera, effects) · `crates/crabrig` crab puppet ·
`crates/painter` oil painter (`crabpaint`) · `assets/` files the game loads · `art/` source art · `tools/` build,
test and tidy scripts · `web/` the page.

## Keeping the computer happy
Bevy builds are big and teammates can't fix a broken computer.
- `cargo tidy` runs at session start (`.claude/settings.json`) and cleans build files when `target/` passes 8 GB or the
  disk has under 10 GB free.
- Anything broken (disk full, tools missing, git stuck, odd errors)? Run `/doctor`.
- Before a big build, render or video job, check space with `cargo tidy report`; stop under ~10 GB free.
- Delete your scratch files and merged worktrees before you finish.
