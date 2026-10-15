# Helping with CrabScape

You don't need to know how to code or design games. Claude does the designing and building. What we need from you
is **taste**: play the builds, look at the art, and say honestly what feels good and what feels off. If you want,
you can also lend your Claude Code to do small tasks.

CrabScape is all rights reserved: there is no open-source licence, so you're welcome to look and help, but the code
and art can't be reused elsewhere.

## 1. Install (once)
- **git**: <https://git-scm.com/downloads>
- **GitHub CLI (`gh`)**: <https://cli.github.com>, then run `gh auth login` and follow the prompts.
- **Claude Code**: <https://claude.com/claude-code>
- **Rust** (only if you want to build and run the game on your own computer): <https://rustup.rs>

Ask the owner (StevenKauwe) to add you as a collaborator first, then accept the email invite.

## 2. Get the project
```sh
gh repo clone StevenKauwe/crabscape
cd crabscape
claude
```

## 3. Do a task with your Claude
Inside Claude Code, type:
```
/next-task
```
Claude asks how much budget you have, picks a small task that fits, claims it so nobody else does it twice, does the
work and opens a pull request. You don't need to read the code: automatic checks test it, and another person's
Claude reviews it (`/review-pr <number>`).

**Your Claude's token budget:** pick **S** tasks when you're running low (about 30 minutes of Claude's work).
Pick **M** only when you have plenty left. Tasks marked `needs-human` wait for a person's decision or drawing.

If your Claude runs out halfway, that's fine: it leaves a note on the task so someone else can continue.

## 4. Give taste feedback (the most useful thing you can do)
- Play the latest build (the link is in the README) on your phone or computer.
- Open a new issue and choose **Taste feedback**: <https://github.com/StevenKauwe/crabscape/issues/new/choose>
- Say what you tried, what felt off, and what felt great. Short and honest is perfect. No technical words needed.

The lead's Claude reads feedback (`/triage`) and turns it into tasks.

## House rules, in plain words
- Everything in the game works like the real ocean. Crabs breathe water, so no air tanks.
- Small changes, one task at a time.
- Be kind. It's a game among friends.
