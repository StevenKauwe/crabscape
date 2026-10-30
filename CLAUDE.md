# CrabScape: notes for Claude

A cozy, brutal, surreal, biologically true side-scrolling game: you are a heavy, lantern-carrying crab on the deep-sea
floor. Rust + Bevy. "Feels like Valheim. Plays like Silksong. Grows like RuneScape." See `docs/design/vision.md`.

The team is a few friends with small token budgets. Humans play, give taste and look at art; Claude designs, builds
and runs all process. Nobody reads code, so tests and CI are the safety net.

## Anchors (read the ones your task touches)
- Style and art direction: `docs/design/art-direction.md` (crab anatomy: `art/crab/anatomy-notes.md`)
- Tone, pillars, loop: `docs/design/vision.md`
- Ocean-shaped rules (biology, no air, real mechanisms only): `docs/design/ocean-shaped.md`
- Zooming out shows the crab's memory, not the world: `docs/design/crab-memory.md`
- How it plays (movement, combat, controls): `docs/design/how-it-plays.md`
- World, zones, economy: `docs/design/world.md` (ecology by depth, food web, creature rules: `docs/design/universe.md`)
- Enemies (biology, combat spec, rig notes): `docs/design/creatures/` (concept sheets: `art/creatures/`)
- Co-op promises (what playing together guarantees, and the checks): `docs/design/coop-promises.md`
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
  deterministic. Co-op (`crates/sim/src/net.rs`): each device runs its own crab, the defender decides blows, the
  host's browser runs the world. Anything that changes a crab happens only on its own device; skip `remote` crabs.
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
- **When a human should try it first.** If you judge a change needs a person to play it before it lands (how it
  feels or looks), post its playable preview link to the owner in plain words with a recommendation, and wait for
  their go. Otherwise merge as above.
- **Decide; don't wait on a human.** Every technical and game-rule call is Claude's: code, testing, CI, co-op and
  connections, and rules such as what's shared in co-op or how a mechanic works. Pick a sensible answer, record it
  in `docs/design/decisions.md` with a one-line reason, and mention it afterwards in one plain line anyone can
  overrule. Choosing and being wrong, then fixing it later, beats waiting (the owner's call: the team is here for
  taste, not game-dev answers). This goes for helpers too: brief them to decide, not to ask.
- **Ask humans only** for taste (how it feels or looks) or outward-facing calls (going public, money, contacting
  people): briefly, in plain words, with a recommendation.
- **Talking to humans:** plain words, no jargon (say "your change", "the automatic checks", "it's in the game now").
  Anything a human must do comes as one of: a yes/no or one `! command` in chat, a button on an artifact page (Studio:
  https://claude.ai/artifact/PttxVTZCC75dqSEpKXYowf), or one link that lands on the action. Never "go to Settings…".
- **Taste input** comes from the Studio page and `feedback` issues; `/triage` turns it into tasks.
- **Always start from the latest `main`.** `git fetch origin` and branch (or make your worktree) from `origin/main` before any change; fetch and rebase again right before pushing.
- **One PR per feature, moderate size, one issue each** (owner's call): bundle a feature's parts (rules,
  connection, buttons, deploy) so it can be play-tested working together, rather than thin layers that each do
  nothing yet; split by purpose into commits inside it. Work in your own git worktree when other sessions may be
  active.
- **Respect the token budget:** pick tasks that fit, stop at a clean point, leave a handoff comment if unfinished.
- **Progress over perfection.** Nothing is precious; anything can be changed later by anyone's Claude.
- **Idea to deployed, as fast as possible; nothing is sacred** (owner's call: "in a month, if the game is a
  cow-farming simulator, I'd be happy"). We never care about breaking changes or keeping past versions working:
  - **Co-op:** both players must be on the same version; the older one reloads into the new one. Never keep co-op
    messages readable by an older game, and never test against one.
  - **Saves and export codes:** when a change would break a saved crab, write a migration (a small function from the
    old shape to the new). No `#[serde(default)]` habit, no "saves from before…" special cases kept around forever;
    once nobody can still hold an old save, its migration can go too.
  - **Anything else** (formats, files, tools, rules, the game itself): change it outright. No shims, flags or
    deprecation steps.
- New or changed decisions go in `docs/design/decisions.md`.

## Repo practices and CI/CD
- Branch (`issue-<n>-<slug>`) → PR (`Closes #<n>`, what changed, how verified, screenshots for visual changes) →
  `gh pr merge --squash --auto --delete-branch`. Rebase on `main` and resolve conflicts yourself. Never push to `main`.
- CI (`.github/workflows/ci.yml`, Rust pinned in `rust-toolchain.toml`): fmt, clippy (native + wasm32), tests,
  cargo-deny, the tidy tool's checks, mutation testing of a PR's changed lines (and on `main`, a coverage report: see
  "Testing and linting" below), the web build, relaxed-SIMD check, a size report, the browser tests (after the web
  build, at once: `cargo webtest` portrait and landscape in WebKit, `cargo cooptest` in three parts, frame strips;
  the tools are built once beside the web build), native = wasm determinism (`tools/determinism.sh`, the sim's tests in wasmtime too) and an art
  check (re-exports the rigs and oil atlases; the committed crab and creature art must match; on a Mac, take the
  oil atlases from its `art-as-ci-made-it` download). On `main` it deploys
  `web/` to GitHub Pages (https://stevenkauwe.github.io/crabscape/), then checks the live site starts
  (`cargo webtest --smoke`). Branch protection requires the check, determinism, art, web and browser jobs.
- **CI runs what a PR's files can affect** (`tools/changes.sh`, the `changes` job): words only (`*.md`, `docs/`,
  `.claude/skills/`) skip every heavy job and pass in seconds; the art check runs only for the art pipeline
  (`crates/crabrig`, `crates/painter`, `tools/oil-atlas`, `tools/paint-oil.sh`, `art/`, `assets/`), Cargo files, the
  toolchain or CI itself; anything the script doesn't recognise runs everything. Skipping is by job-level `if:` (a
  skipped required job counts as passed; never use workflow `paths` filters for required jobs: they never report
  and block the merge). Pushes to `main` always run everything. A new kind of file? Add it to `tools/changes.sh`.
- **Main is broken?** A failed CI run on `main` opens (or adds to) the `main-broken` issue "main is broken"; a green
  run closes it. Fix it before anything else (`/next-task` picks it first; the session-start check says so too).
- **Playable previews** (`tools/previews.sh`): CI publishes each PR's web build to `/pr-<n>/` on the Pages site
  (link commented on the PR, refreshed per push; `previews.yml` removes it on close); a named branch on demand with
  `gh workflow run ci.yml --ref <name>` to `/branches/<name>/`; `/builds/` lists them all. Each build keeps its own
  save on the device.
- Issues: `claude-ready`, `size:S` (~30 min of Claude work), `size:M` (~1 h); `size:L` must be split; `needs-human`
  means a person must decide or draw first (don't start it). Areas: `area:code|art|research|design|tooling`.
- Skills: `/next-task` (claim, do and merge one issue), `/review-pr <n>` (optional self-check), `/triage`
  (feedback → tasks), `/doctor` (fix a broken computer or project copy).

## Setup and commands
Setup: Rust from https://rustup.rs (it installs the pinned version and targets in `rust-toolchain.toml` by itself),
`cargo install wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>`, optionally `cargo install wasm-tools`
and binaryen's `wasm-opt`. Linux also needs `libudev-dev libwayland-dev libxkbcommon-dev pkg-config`.

| Job | Command |
|---|---|
| Run the game (native) | `cargo run` |
| Format | `cargo fmt` |
| Lint (native) | `cargo clippy --workspace --all-targets -- -D warnings` |
| Lint (wasm32) | `cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings` |
| Test | `cargo test --workspace` |
| Property tests (deeper search) | `PROPTEST_CASES=2000 cargo test -p crabsim --release --test properties` |
| Dependencies (advisories, licences, duplicates, sources) | `cargo deny --workspace --locked check`, then the same with `--target wasm32-unknown-unknown` (and `deny.toml` has the introducer's) |
| Mutation testing of your change | `tools/mutants-pr.sh` (changed lines since `origin/main`; needs `cargo install --locked cargo-mutants`) |
| Coverage report | `cargo llvm-cov --workspace --summary-only` (needs `cargo install --locked cargo-llvm-cov` and `rustup component add llvm-tools`) |
| Web build into `web/` | `tools/build-web.sh` (serve with any static server, e.g. `python3 -m http.server -d web`) |
| WebKit (iPhone engine) test | `cargo webtest [--landscape \| --desktop] [--shots /tmp/shot]` (after the web build; needs WebKitGTK 4.1 and `WebKitWebDriver`, Ubuntu `webkitgtk-webdriver`, before 26.04 `webkit2gtk-driver`) |
| Co-op promises in real browsers | `cargo cooptest [--only join,guest-reload] [--shard I/N] [--repeat N] [--voice on\|off\|both]` (after the web build and `cargo build --release --manifest-path tools/introducer/Cargo.toml`; needs Chrome). `--stress [--seed S] [--steps K]` does random reloads, Start overs, stalls and introducer restarts (a failure prints its replay line); `--url <site>` plays a deployed site with the real introducer; `--list` names the scenarios (`docs/design/coop-promises.md`) |
| Headless sim | `cargo crabsim --script "t=0 walk=right; t=1 jump; t=1.45 jump" --seconds 4` |
| Replay a bug report | `cargo crabsim --inputs replay.txt --dump -` (F9 in the game saves a replay; `?replay` adds a button on the web) |
| Crab rig → SVG | `cargo run -p crabrig --profile art -- parts` (writes `art/crab/crab-parts.svg`) |
| Rig → game atlas | `cargo run -p crabrig --profile art -- export` (writes `assets/crab/`) |
| Oil-paint the atlases (crab and creatures) | `tools/paint-oil.sh [crab \| <creature>...]` |
| Oil painter | `cargo run -p painter --profile art --bin crabpaint -- oilphoto in.png out.png` |
| Disk housekeeping | `cargo tidy` (`cargo tidy report` only looks) |
| Co-op introducer | `cargo run --release` in `tools/introducer`, then the game with `?introducer=ws://localhost:3536`. Live: one Fly.io machine, redeployed by `introducer.yml` on `main` (setup and rules in its `src/main.rs`). It also files player reports (`POST /report`, `src/report.rs`) when the `GITHUB_REPORT_TOKEN` secret is set; locally, `GITHUB_REPORT_TOKEN=$(gh auth token)` files a real issue (close it after) |
| Spoken reports | `crates/listen` (Moonshine speech to text, built into `web/listen/` by `tools/build-web.sh`). With the model's files in a folder and a 16 kHz WAV: `LISTEN_MODEL=<folder> LISTEN_CLIP=<wav> cargo test -p listen --release -- --ignored`. In headless Chrome, `--use-file-for-fake-audio-capture=<plain wav>` speaks into the microphone, but only with `--disable-features=AudioServiceSandbox,AudioServiceOutOfProcess` (the sandboxed audio service can't read the file) |
| Co-op on one computer | Two windows side by side (a hidden tab pauses). A browser that hides local addresses (Chrome's mDNS, often without macOS Local Network access) can't connect them: use headless Chrome with `--disable-features=WebRtcHideLocalIpsWithMdns --disable-renderer-backgrounding --disable-backgrounding-occluded-windows` and each player in its own window (DevTools `Target.createTarget` with `newWindow`), or a phone on cellular. Console lines starting `co-op:` say who connected, met, went silent or rejoined. `data-coop` on the canvas says each tab's co-op status. Voice: add `--use-fake-device-for-media-stream --use-fake-ui-for-media-stream --autoplay-policy=no-user-gesture-required --mute-audio` (a fake beeping mic, kept off the speakers); `data-voice` says the mic, each friend's call and how many friends are heard |

Feel change? Run the same crabsim script on `main` and on your branch and compare the numbers (`cargo crabsim --help`
lists the script format and flags). Visual change? Attach before/after screenshots.

## Testing and linting
What each automatic check is for (commands in the table above; all in CI):
- **Lints:** clippy's defaults plus `[workspace.lints]` in `Cargo.toml` (every crate takes them on with
  `[lints] workspace = true`): no `unsafe` code, no `dbg!`/`todo!`/`unimplemented!`. `crates/sim` also forbids
  `unwrap()` outside tests (a panic there stops every player's game): handle the `None`/`Err`, or `expect` with the
  reason it can't happen. Native and wasm32, `-D warnings`.
- **Tests prove promises:** scenario tests for feel and rules, and **property tests**
  (`crates/sim/tests/properties.rs`, proptest) for promises that hold for any input: the crab stays in the level with
  finite numbers, recordings replay exactly, same inputs give the same world, bytes from another device and replay
  text never crash. Also run as WebAssembly (in wasmtime) by the determinism job.
- **cargo-deny** (`deny.toml`): fails on a known security advisory, a licence not on the list, or an unknown source;
  warns on duplicate crate versions (each is downloaded twice in the web build). An ignored advisory says why it
  can't reach us. Our own crates have no licence and are skipped (never add one).
- **Mutation testing on the changed lines** (`mutation testing (changed lines)`, `tools/mutants-pr.sh`,
  `.cargo/mutants.toml`): cargo-mutants breaks each changed line of `crates/sim`, `src/coop.rs`, `src/voice.rs`,
  `src/character.rs` and `tools/introducer` in small ways; the job fails if no test notices. Fix it with a test of
  the behaviour the line is for. Browser-only code (`web::`), Bevy plugin wiring, on-screen layout and the `crabsim`
  command line are left out. A weekly full run of `crates/sim` (`mutants-weekly.yml`) lists the old gaps, for
  information only.
- **Coverage** (`test coverage (information only)`, `coverage.yml`): cargo-llvm-cov's table in the job summary, after
  each push to `main` and weekly, not on PRs. No threshold and it never fails: mutation testing is what holds new code
  to its tests.

**CI hygiene** (owner's rule: "testing and CI/CD hygiene are really important, so that changes are buttery smooth and
fast"):
- **Time budgets:** a code change's required checks finish in about 8 minutes wall time (7 is the target); a
  docs-only change in under a minute. A PR that makes CI slower says so and why.
- **Independent work runs in parallel**, on separate jobs; nothing waits that doesn't depend on something (the
  browser tests fan out after the web build, and the required `browser test (WebKit, iPhone size)` is a gate over
  them).
- **Tests are hermetic and idempotent:** their own ports, profiles, temp dirs and seeds; no shared state; the same
  seed gives the same result; safe to run side by side and to re-run.
- **Flaky means broken:** no automatic retries. A flaky test is fixed, or quarantined with an issue, never ignored.
- **Run what a change needs** (`tools/changes.sh`); `main` runs everything as the safety net.
- **Long local jobs are bounded:** a time limit, and one blocking wait (`gh run watch <id> --exit-status`), never
  repeated short polls.

Layout: `crates/sim` rules · `src/` the Bevy game (drawing, input, camera, effects) · `crates/crabrig` crab puppet ·
`crates/painter` oil painter (`crabpaint`) · `crates/listen` speech to text for spoken reports · `assets/` files the
game loads · `art/` source art · `tools/` build, test and tidy scripts · `web/` the page.

## Keeping the computer happy
Bevy builds are big and teammates can't fix a broken computer.
- `cargo tidy` runs at session start (`.claude/settings.json`) and cleans build files when `target/` passes 8 GB or the
  disk has under 10 GB free.
- Anything broken (disk full, tools missing, git stuck, odd errors)? Run `/doctor`.
- Before a big build, render or video job, check space with `cargo tidy report`; stop under ~10 GB free.
- Delete your scratch files and merged worktrees before you finish.
