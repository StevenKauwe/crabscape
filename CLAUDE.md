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
`src/` game code (`tuning.rs` holds every feel number) · `assets/` files the game loads · `art/` source art
(`art/crab/crab-parts.svg` is the rig, `anatomy-notes.md` the sourced anatomy) · `concept/` sketches · `docs/` design,
poster, teaser scripts, screenshots · `tools/` build and test scripts · `web/` the page (`pkg/`, `assets/` are built).

## Checks (run all before a PR; CI runs them too)
```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --target wasm32-unknown-unknown -- -D warnings
cargo test
tools/build-web.sh                 # web build + relaxed-SIMD check + size report
python3 tools/test-webkit.py       # optional, needs WebKitGTK: plays it at iPhone size, saves screenshots
```
Visual change? Attach before/after screenshots (`python3 tools/test-webkit.py --shots /tmp/shot`) to the PR.

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
