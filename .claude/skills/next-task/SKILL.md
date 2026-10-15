---
name: next-task
description: Pick the best open CrabScape issue for this person's token budget, claim it, do it on a branch, run the checks and open a PR. Use when someone says "next task", "give me something to do", or types /next-task.
argument-hint: "[S|M] [code|art|research|design|tooling]"
---

# /next-task

Arguments (optional): `$ARGUMENTS`, a size budget (`S` or `M`) and/or an area. If neither is given, ask once:
"How much budget do you have: S (~30 min of my work) or M (~1 h)? Any area you prefer?" Low budget → `S`.

## 1. Get oriented (cheap)
- Read `CLAUDE.md`. Read only the `docs/design/*.md` pages the chosen issue touches.
- `git status` must be clean. `git switch main && git pull --ff-only`.
- `gh auth status` must work. Note the GitHub login: `gh api user --jq .login`.

## 2. Pick
```sh
gh issue list --state open --label claude-ready --search "no:assignee -label:in-progress -label:needs-human" \
  --json number,title,labels --limit 50
```
Filter by `size:<budget>` (an `S` budget takes only `size:S`; `M` takes `size:S` or `size:M`) and by area if given.
Never pick `size:L` or `needs-human`. Prefer: lowest number in the best-fitting area, then anything that unblocks
other issues. Read the chosen issue in full (`gh issue view <n> --comments`). If it is unclear or too big, comment
why, skip it and pick another. If nothing fits, say so and stop (don't invent work).

Tell the person in one line which issue you picked and why, then claim it:
```sh
gh issue edit <n> --add-assignee @me --add-label in-progress
gh issue comment <n> --body "Claimed by <login>'s Claude."
```

## 3. Do the work
- Branch: `git switch -c issue-<n>-<short-slug>`.
- Stay inside the issue's "Done when". Don't refactor neighbours, don't add dependencies without saying why.
- Follow the rules in `CLAUDE.md` (ocean-shaped, no licence, no relaxed-SIMD, no generated junk in git).
- Research tasks: real, opened sources only, with links. Mark anything you couldn't open as "not verified".
- If it is growing past its size, stop: commit what's useful, comment on the issue with findings and a proposed split.

## 4. Check
Run every check in `CLAUDE.md` that applies (fmt, clippy native + wasm32, tests; `tools/build-web.sh` for anything
that changes the game). Fix failures. For visual changes take before/after screenshots
(`python3 tools/test-webkit.py --shots /tmp/issue-<n>` if WebKitGTK is installed; otherwise describe what to look at).

## 5. Open the PR (never merge it)
```sh
git push -u origin HEAD
gh pr create --title "<what it does> (#<n>)" --body "$(cat <<'EOF'
Closes #<n>

## What changed
- ...

## How I verified it
- cargo fmt / clippy (native + wasm32) / test: pass
- ...

## Screenshots
(for visual changes; drag images into the PR on github.com if they can't be attached here)
EOF
)"
```
Commit messages: short summary line, then why. Then tell the person: the PR link, what a human should look at (a
taste check?), and that another person's Claude can review it with `/review-pr <pr-number>`.
