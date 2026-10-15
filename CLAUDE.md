# CrabScape: notes for Claude

A side-on 2D browser game in Rust + Bevy (wasm, WebGL2). You play a heavy, lantern-carrying crab on the deep-sea
floor. "Feels like Valheim. Plays like Silksong. Grows like RuneScape." Solo is the full game; P2P co-op comes later.
Milestone 1 is one test level and one crab (see README.md).

**Team:** a few friends, no game designers. Claude is the designer and builder. Humans give taste (play builds, react
to art, vote) and run Claude on small token budgets. Nobody reads code, so checks are the safety net. Keep work small.

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
- Use the shared skills: `/next-task` (claim and do one issue), `/review-pr <n>` (cross-review), `/triage` (lead only).
- One issue per branch (`issue-<n>-<slug>`) and per PR. Keep PRs small. PR body has `Closes #<n>`, what changed,
  how you verified it, screenshots for visual changes. Never merge your own PR; never push to `main`.
- If the work turns out bigger than the size label, stop, comment on the issue with what you learned, and suggest
  a split instead of burning budget.
- Decisions go in `docs/design/decisions.md` (dated). Don't change a rule above without the owner (StevenKauwe).
