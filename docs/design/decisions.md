# Decisions

The current decisions. One row each: date, decision, who decided. When a decision changes, edit its row (with the new
date) instead of adding a contradicting one. "Owner" = StevenKauwe.

| Date | Decision | By |
|---|---|---|
| 2026-10-14 | Rust + Bevy; WebAssembly in the browser (WebGL2) and native on PC; must run on iPhone (WebKit); no relaxed-SIMD ever | Owner |
| 2026-10-14 | Co-op later via matchbox (WebRTC), host-authoritative, 2–8 players, no database, cheating allowed; saves live in each player's browser | Owner |
| 2026-10-14 | Solo is the full game; RuneScape-style economy where rarer resources come from deeper zones; trading optional (Ironman-style solo) | Owner |
| 2026-10-14 | A hosted shared-world server is far off: refactor then, don't design for it now | Owner |
| 2026-10-15 | Pillars: cozy, brutal, surreal. Hook: "Feels like Valheim. Plays like Silksong. Grows like RuneScape." | Owner + Claude |
| 2026-10-15 | Side-on 2D camera with parallax | Claude (owner asked for value calls) |
| 2026-10-15 | Look: hand-drawn ink + painted fills, oil-painted assets; per-shape variation, never full-frame filters | Claude, owner's taste |
| 2026-10-15 | Why dive: to grow (bigger shell-home) + the trench mystery. Surface: beach/tide-pool hub. Death: Valheim-style, loot stays in your cast-off shell. Neighbours: a few, the whale-fall village | Claude (owner asked for value calls) |
| 2026-10-15 | **Ocean-shaped:** everything biologically correct; surreal = the real ocean at crab scale; grounded fantasy allowed; game magic only through real mechanisms; self-consistency never bends; facts need real sources | Owner |
| 2026-10-15 | Crab breathes water: no air tank, no air bladder, no "brewing air". Pressure via body chemistry and adapting, not helmets. Lantern = farmed luminous bacteria (yellow Y-1-like strain) | Owner + Claude |
| 2026-10-15 | Art highly stylised for emotion; accuracy lives in anatomy and behaviour | Owner |
| 2026-10-15 | Feel: Silksong-like moves, but a heavy-hitting tank (momentum, power, timing, anticipation), never sluggish; Expedition 33-style timed parries and counters; enemy tells from real behaviour | Owner |
| 2026-10-15 | Player crab: composite swimming crab with paddle 5th legs (jump/double jump/swim), crusher on its right (screen left), cutter on its left, front view in play | Claude, owner confirmed paddles |
| 2026-10-15 | Paint ahead, play live: offline oil-painted lit + unlit sprites blended at runtime | Owner |
| 2026-10-15 | Dodge goes the way you hold and needs a direction; dodge alone does nothing. A 0.1 s buffer lets dodge come a moment before the direction, landing or the end of the cooldown (#32) | Koa (playtest) |
| 2026-10-15 | No licence: all rights reserved | Owner |
| 2026-10-15 | Humans only give taste (Studio page, demos); Claude does all work and process, merges its own PRs once CI passes, and asks humans only for taste/direction or outward-facing calls. GitHub issues are the task list, sized to token budgets | Owner |
| 2026-10-16 | 100% Rust: game, tools and art pipeline (`crates/painter` = the owner's oil painter `crabpaint`, `crates/crabrig` = the crab puppet); no JS, Python or Node in the pipeline | Owner |
| 2026-10-16 | Game rules live in the deterministic, headless `crabsim` crate (`crates/sim`, fixed 1/120 s ticks driven only by commands); the Bevy game ticks and draws it; `cargo crabsim` runs it for tests, replays and later bots and the co-op host | Claude |
| 2026-10-16 | The game ships on GitHub Pages from `main`; no per-file size cap, but download size still matters | Owner |
| 2026-10-16 | Docs hold only the current state; history lives in git | Owner |
| 2026-10-16 | Swimming takes stamina: paddle strokes one at a time with a rhythm (no hovering), stamina for 3 strokes to start (meant to grow with use), refilled only on solid ground; shown as small rings over the crab only while tired. Opens up vertical, hidden routes and makes the way back up the cost of a dive (#37) | Koa (taste) |
| 2026-10-16 | Sink, don't drop: a hard push-off the water brakes, then a slow, steady sink (no extra fall pull; ~0.8 s down from a ~0.5 s rise, top sinking speed ~350). The trench widened to 480 so it still needs a stroke (#39) | Koa (taste) |
