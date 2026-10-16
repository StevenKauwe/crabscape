---
name: review-pr
description: Optional Claude-only check of a CrabScape pull request - your own before you merge it, or already-merged work when you have spare budget. Never a human step; no human approval is ever needed. Use for /review-pr <number> or "check PR 12".
argument-hint: "<pr-number>"
---

# /review-pr (optional, Claude-only)

PR number: `$ARGUMENTS` (if missing: your own open PR, else a recent one from `gh pr list --state merged --limit 5`).
This is a self-check, not a gate. Nobody approves anything, and never ask a human to review code.

1. **Context.** Read `CLAUDE.md`. `gh pr view <n> --comments` and the linked issue's "Done when".
2. **Checks.** `gh pr checks <n>`. If CI is missing or red, `gh pr checkout <n>` and run the checks in `CLAUDE.md`.
3. **Read the diff** (`gh pr diff <n>`). Look for, in this order:
   - Does it do what the issue's "Done when" says, and nothing unrelated?
   - Rule breaks: licence files/fields, relaxed-SIMD or `RUSTFLAGS` tricks, air/air-tank/pressure-helmet ideas,
     invented or unopened citations, full-frame filters, a file over 15 MiB, generated junk (`target/`, `web/pkg/`,
     renders, videos), secrets or personal paths.
   - Bugs: panics on the web, frame-rate dependent physics (must use the fixed step), anything moving the crab
     directly instead of through `MoveRequest`.
   - Movement feel numbers belong in `crates/sim/src/tuning.rs` with a comment; game rules belong in `crates/sim`
     (deterministic: no clocks, no unseeded randomness), never in Bevy systems.
4. **Act, cheaply.** Your own open PR: fix what you found, then merge as in `/next-task`. Another Claude's open PR:
   a short comment (file:line, why, fix); don't push to their branch. Merged work: fix small problems in a new small
   PR you merge yourself, or open a `claude-ready` issue for bigger ones. Skip nits.
