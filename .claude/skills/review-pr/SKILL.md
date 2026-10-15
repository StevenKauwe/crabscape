---
name: review-pr
description: Cross-review someone else's CrabScape pull request - run the checks, read the diff against the CLAUDE.md rules, comment findings, approve only if everything passes. Use for /review-pr <number> or "review PR 12".
argument-hint: "<pr-number>"
---

# /review-pr

PR number: `$ARGUMENTS` (if missing, list `gh pr list --search "review:required -author:@me"` and ask which).

1. **Context.** Read `CLAUDE.md`. `gh pr view <n> --comments` and the linked issue's "Done when". Never review your
   own PR (the author can't approve it anyway).
2. **Checks.** `gh pr checks <n>`. If CI is green you may skip re-running locally; if CI is missing or red,
   `gh pr checkout <n>` and run the checks in `CLAUDE.md` yourself.
3. **Read the diff** (`gh pr diff <n>`). Look for, in this order:
   - Does it do what the issue's "Done when" says, and nothing unrelated?
   - Rule breaks: licence files/fields, relaxed-SIMD or `RUSTFLAGS` tricks, air/air-tank/pressure-helmet ideas,
     invented or unopened citations, full-frame filters, a file over 15 MiB, generated junk (`target/`, `web/pkg/`,
     renders, videos), secrets or personal paths.
   - Bugs: panics on the web, frame-rate dependent physics (must use the fixed step), anything moving the crab
     directly instead of through `MoveRequest`.
   - Feel numbers belong in `src/tuning.rs` with a comment.
4. **Report.** Keep it short and concrete (file:line, why, suggested fix).
   - Problems: `gh pr review <n> --request-changes --body "..."`.
   - Small nits only: `gh pr review <n> --comment --body "..."`.
   - All good and checks pass: `gh pr review <n> --approve --body "Checks pass; matches the issue. <one-line summary>"`.
   - A visual or feel change also needs a human taste verdict: say so in the review and leave merging to a person.
5. Never merge. Never push to the author's branch unless they asked.
