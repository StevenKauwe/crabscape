---
name: next-task
description: Pick the best open CrabScape issue for this person's token budget, claim it, do it on a branch, run the checks, open a PR and merge it yourself when checks pass. Use when someone says "next task", "give me something to do", or types /next-task.
argument-hint: "[S|M] [code|art|research|design|tooling]"
---

# /next-task

Arguments (optional): `$ARGUMENTS`, a size budget (`S` or `M`) and/or an area. If neither is given, ask once, in
plain words: "Do you have a little time (about half an hour of my work) or more (about an hour)? Anything you'd like
to work on?" Low budget or no answer → `S`. Talk without jargon (see "Talking to humans" in `CLAUDE.md`); you do
every GitHub/git step yourself.

## 1. Get oriented (cheap)
- Read `CLAUDE.md`. Read only the `docs/design/*.md` pages the chosen issue touches.
- `git status` must be clean. `git switch main && git pull --ff-only`.
- `gh auth status` must work (if not: ask them to type `! gh auth login` and follow its prompts). Note the login:
  `gh api user --jq .login`.

## 2. Pick
**A broken `main` comes first.** CI opens (and closes, once green) one issue titled "main is broken":
```sh
gh issue list --state open --label main-broken --json number,title,assignees
```
If it is open and nobody is on it, take it whatever the budget or area: it names the failing run and step. Fix
main (or revert the change that broke it) before anything else. Otherwise pick normally:
```sh
gh issue list --state open --label claude-ready --search "no:assignee -label:in-progress -label:needs-human" \
  --json number,title,labels --limit 50
```
Filter by `size:<budget>` (an `S` budget takes only `size:S`; `M` takes `size:S` or `size:M`) and by area if given.
Never pick `size:L` or `needs-human`. Prefer: lowest number in the best-fitting area, then anything that unblocks
other issues. Read the chosen issue in full (`gh issue view <n> --comments`). If it is unclear or too big, comment
why, skip it and pick another. If nothing fits, say so and stop (don't invent work).

Tell the person in one plain line which task you picked and why, then claim it:
```sh
gh issue edit <n> --add-assignee @me --add-label in-progress
gh issue comment <n> --body "Claimed by <login>'s Claude."
```

## 3. Do the work
- Branch: `git switch -c issue-<n>-<short-slug>`.
- Stay inside the issue's "Done when". Don't refactor neighbours or gold-plate; don't add dependencies without
  saying why.
- Follow the rules in `CLAUDE.md` (ocean-shaped, no licence, no relaxed-SIMD, no generated junk in git).
- Research tasks: real, opened sources only, with links. Mark anything you couldn't open as "not verified".
- Growing past its size or the budget? Stop at a clean point: commit what's useful, leave a short handoff comment on
  the issue (done, left, proposed split), and unassign yourself.

## 4. Check
Run every check in `CLAUDE.md` that applies (fmt, clippy native + wasm32, tests; `tools/build-web.sh` for anything
that changes the game). Fix failures. For visual changes take before/after screenshots
(`cargo webtest --shots /tmp/issue-<n>` if WebKitGTK and its WebDriver are installed; otherwise describe what to look at).

## 5. Open the PR and merge it yourself
No human review or approval is needed. If budget allows, run `/review-pr` on your own PR first (optional).
```sh
git fetch origin && git rebase origin/main     # resolve conflicts yourself, re-run checks if anything changed
git push -u origin HEAD --force-with-lease
gh pr create --title "<what it does> (#<n>)" --body-file <body.md>
gh pr merge --squash --auto --delete-branch     # merges when CI passes; `Closes #<n>` closes the issue
```
PR body: `Closes #<n>`, what changed, how you verified it, screenshots for visual changes. Commit messages: short
summary line, then why. If CI fails, fix and push. If `main` moves and conflicts, rebase and resolve; the auto-merge
stays queued.

Then tell the person in plain words what's now in the game (or what's left). For a look or feel change, ask for their
reaction on the Studio page: https://claude.ai/artifact/PttxVTZCC75dqSEpKXYowf. Never ask them to check code.
