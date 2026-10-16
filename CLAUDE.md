# CrabScape: notes for Claude

A side-on 2D browser game in Rust + Bevy (wasm, WebGL2). You play a heavy, lantern-carrying crab on the deep-sea
floor. "Feels like Valheim. Plays like Silksong. Grows like RuneScape." Solo is the full game; P2P co-op comes later.
Milestone 1 is one test level and one crab (see README.md).

**Team:** a few friends, no game designers. Claude is the designer and builder. Humans run Claude on small token
budgets. Nobody reads code, so checks are the safety net.

## How we work
- **Humans bring taste; Claude does the work, including all process.** Never ask a human to review code, approve a
  PR, resolve a merge conflict, or do git chores.
- **Merge your own PR** when your task's checks pass: `gh pr merge --squash --auto --delete-branch`. Small PRs, one
  issue each. Rebase on `main` before merging; if `main` moved and things conflict, resolve them yourself.
- **Nothing is precious.** Anything merged can be changed later by anyone's Claude. Progress over perfection; don't
  gold-plate.
- **Only stop and ask a human** for genuine taste or direction calls (how it should feel or look, which way the
  design goes) or anything outward-facing (making things public, spending money, contacting people). Ask briefly,
  in plain words, with a recommended choice.
- **Taste input** arrives via the Studio hub artifact (https://claude.ai/artifact/PttxVTZCC75dqSEpKXYowf) and
  `feedback` issues; `/triage` turns it into tasks.
- **Talking to humans:** most teammates don't know GitHub or dev tools exist. Anything a human truly must do comes in
  one of three forms only: (1) in this conversation: you do it, ask a yes/no, or (only for an unavoidable
  interactive login) have them type one `! command`; (2) a button or form on an artifact page (Studio hub, a demo);
  (3) one clickable link that lands exactly on the action (e.g. https://github.com/StevenKauwe/crabscape/invitations).
  Never "go to Settings → … and click …". No jargon (PR, branch, merge, CI, issue) in anything a human reads: say
  "your change", "the task", "the automatic checks", "it's in the game now".
- **Respect the token budget:** pick tasks that fit, stop at a clean point, and if unfinished leave a short handoff
  comment on the issue.

## Rules that never bend
- **No licence.** All rights reserved. Never add a LICENSE file or licence fields.
- **Ocean-shaped.** Everything is biologically real. Surreal = the real ocean seen at crab scale. Grounded fantasy is
  OK (bacteria lantern, litter as gear, one crab mixing several species' behaviours). Game "magic" (poison, stun,
  blind...) only through a real mechanism, and the same rules apply to everyone. **Self-consistency never bends.**
- **Crabs breathe water.** No air, air tanks, air bladders or "air brews". Pressure is handled by body chemistry
  (e.g. TMAO) and adapting over time, never helmets or armour. The lantern is farmed glowing bacteria.
- **Facts need real sources.** Never invent a citation. If you can't open a source, mark the fact "not verified".
- **Art:** stylised for emotion, accurate in anatomy and behaviour. Hand-drawn ink + painted fills, oil-painted
  assets. Variation lives in each shape; **never full-frame filters or overlays**. "Tame but bold."
- **Feel:** Silksong-like moves but a heavy-hitting tank: momentum, power, timing, anticipation. Never sluggish.
  Timed parries and counters. Enemy tells come from the animal's real behaviour.
- **iPhone (WebKit) must work:** WebGL2 only, **no relaxed-SIMD ever** (`wasm-tools validate --features=-relaxed-simd`).
  Keep `RUSTFLAGS` empty for wasm builds.
- **Files:** each served file (the `.wasm` especially) must stay under **15 MiB** (claude.ai artifact limit).
- **Never commit generated junk:** `target/`, `web/pkg/`, `web/assets/`, renders, scratch, throwaway art iterations,
  videos. Big binaries bloat every clone forever.

Read the one-pagers in `docs/design/` that touch your task before you start (`vision`, `ocean-shaped`,
`art-direction`, `how-it-plays`, `world`, `decisions`).

## Where things live
`crates/sim` the simulation: game rules with no graphics (physics, level shape, `tuning.rs` holds every movement
feel number, replays) · `src/` the game: drawing, animation, input, camera, effects (`tuning.rs` there is camera and
light) · `assets/` files the game loads · `art/` source art
(`art/crab/crab-parts.svg` is the rig, written by `crates/crabrig`; `anatomy-notes.md` the sourced anatomy) ·
`concept/` sketches · `docs/` design, poster, teaser scripts, screenshots · `crates/` also has the art tools, all pure
Rust (`painter`: the owner's oil painter and p5.brush port, `crabpaint`; `crabrig`: the crab puppet and parts
exporter; art pipeline is `cargo run`, see README "Changing the crab's art") · `tools/` build and test scripts ·
`web/` the page (`pkg/`, `assets/` are built).

## Checks (run all before a PR; CI runs them too)
```sh
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings      # the game, the sim and the art tools
cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings   # the game and the sim
cargo test --workspace
tools/build-web.sh                 # web build + relaxed-SIMD check + size report
python3 tools/test-webkit.py       # optional, needs WebKitGTK: plays it at iPhone size, saves screenshots
```
Visual change? Attach before/after screenshots (`python3 tools/test-webkit.py --shots /tmp/shot`) to the PR.

## Testing gameplay headless
The game rules are the `crabsim` crate (`crates/sim`): plain Rust, fixed 1/120 s ticks, driven only by controls, no
clocks or randomness, so the same inputs always give bit-identical results. `cargo crabsim` runs it with no window at
about 10 million ticks a second (~80,000x real time). The game is only a presentation of it. Keep it that way: game
rules go in `crates/sim`, never in Bevy systems; anything random in the sim must be a seeded generator stored in `Sim`.
- **Feel changes: measure before and after.** Run the same script on `main` and on your change and compare numbers:
  `cargo crabsim --script "t=0 walk=right; t=1 jump; t=1.45 jump" --seconds 4` prints peak height, event timeline
  (crouch, jump, paddle, landing speed, dodge), end position and a state hash. `--dump states.jsonl` writes every tick
  as JSON (`--every 12` thins it; `--dump -` prints it); `--speed 0.25` paces it in slow motion.
- **Promises go in tests.** `crates/sim/tests/scenarios.rs` plays scripts and checks numbers (jump peaks ~190, the
  trench needs jump + paddle, coyote time, jump buffer, dodge cooldown, replays are exact). If a change should keep a
  feel number, assert it there; if it deliberately changes one, update the test and say so in the PR.
- **Script format** (also replays): entries `t=<s>` or `@<tick>` then `walk=right|left|stop|<-1..1>`, `jump` (press
  and hold), `tap`, `release`, `hold`, `dodge`; `;` or newlines separate; `start x=.. y=..`; `end t=..`. See
  `crates/sim/src/replay.rs`. `cargo crabsim --help` lists everything.
- **Bug reports with replays.** In the game, F9 hands out a replay of the session so far (desktop: a
  `crabscape-replay-*.txt` file; web: a box to copy; add `?replay` to the page address for an on-screen button, e.g. on
  phones). Ask a teammate who says "that felt wrong" to send it; `cargo crabsim --inputs replay.txt --dump -` then
  reproduces it exactly, and it can become a scenario test. A replay warns if the movement tuning has changed since.

## Picking and finishing work
- Tasks are GitHub issues labelled `claude-ready`, sized `size:S` (under ~30 min of Claude work) or `size:M` (~1 h).
  `size:L` must be split first. `needs-human` means a person must decide or draw something: don't start it.
- Use the shared skills: `/next-task` (claim, do and merge one issue), `/review-pr <n>` (optional self-check before
  merging), `/triage` (feedback → tasks).
- One issue per branch (`issue-<n>-<slug>`) and per PR. PR body has `Closes #<n>`, what changed, how you verified
  it, screenshots for visual changes. Changes go through a PR (so CI runs); never push straight to `main`.
- If the work turns out bigger than the size label, stop, comment on the issue with what you learned, and suggest
  a split instead of burning budget.
- Decisions go in `docs/design/decisions.md` (dated). Changing a rule that never bends is a direction call: ask the owner (StevenKauwe).
