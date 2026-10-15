# Decisions log

Newest at the bottom. One line each: date, decision, who decided, and a pointer if there's more. A decision can be
overturned, but only by a new dated entry here (never by quietly changing code or docs). "Owner" = StevenKauwe.

| Date | Decision | By |
|---|---|---|
| 2026-10-14 | Browser game in Rust + Bevy → wasm, WebGL2; must run on iPhone (WebKit); no relaxed-SIMD ever | Owner |
| 2026-10-14 | Co-op later via matchbox (WebRTC), host-authoritative, 2–8 players, no database, cheating allowed; saves live in each player's browser | Owner |
| 2026-10-14 | Solo is the full game; RuneScape-style economy where rarer resources come from deeper zones; trading optional (Ironman-style solo) | Owner |
| 2026-10-14 | A hosted shared-world server is far off: refactor then, don't design for it now | Owner |
| 2026-10-15 | Pillars: cozy, brutal, surreal. Hook: "Feels like Valheim. Plays like Silksong. Grows like RuneScape." | Owner + Claude |
| 2026-10-15 | Side-on 2D camera with parallax (replaces the top-down click-to-move milestone 0) | Claude (owner asked for value calls) |
| 2026-10-15 | Look: hand-drawn ink + painted fills, more detailed than the films; per-shape variation, never full-frame filters | Claude, owner's taste |
| 2026-10-15 | Why dive: to grow (bigger shell-home) + the trench mystery. Surface: beach/tide-pool hub. Death: Valheim-style, loot stays in your cast-off shell. Neighbours: a few, the whale-fall village | Claude (owner asked for value calls) |
| 2026-10-15 | **Ocean-shaped:** everything biologically correct; surreal = the real ocean at crab scale; grounded fantasy allowed; game magic only through real mechanisms; self-consistency never bends; facts need real sources | Owner |
| 2026-10-15 | Crab breathes water: no air tank, no air bladder, no "brewing air". Pressure via body chemistry and adapting, not helmets. Lantern = farmed luminous bacteria (yellow Y-1-like strain) | Owner + Claude |
| 2026-10-15 | Art highly stylised for emotion; accuracy lives in anatomy and behaviour | Owner |
| 2026-10-15 | Feel: Silksong-like moves, but a heavy-hitting tank (momentum, power, timing, anticipation), never sluggish; Expedition 33-style timed parries and counters; enemy tells from real behaviour | Owner |
| 2026-10-15 | Player crab: composite swimming crab with paddle 5th legs (jump/double jump/swim), crusher on its right (screen left), cutter on its left, front view in play | Claude, owner confirmed paddles |
| 2026-10-15 | Oil-painted assets ("better than I'd have ever hoped"). Paint ahead, play live: offline-painted lit + unlit sprites blended at runtime | Owner |
| 2026-10-15 | No licence: all rights reserved | Owner |
| 2026-10-15 | Team workflow: GitHub issues are the task list; small token-sized tasks; CI is the code safety net; humans give taste verdicts on playable builds; decisions land in this file | Owner |
| 2026-10-15 | Working agreement: humans only give taste (Studio hub, docs, demos); Claude does all work and process, merges its own PRs once CI passes (no human review/approval), and asks humans only for taste/direction or outward-facing calls, in plain words via the conversation, an artifact page or one direct link. Nothing is precious: anything merged can be changed later | Owner |
