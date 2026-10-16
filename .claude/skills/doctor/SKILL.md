---
name: doctor
description: Find and fix whatever is making the computer or the project misbehave (full disk, missing or wrong tools, a stuck or messy project copy, sign-in or network trouble), then say in plain words what happened. Use when someone says "something's wrong", "it's broken", "it doesn't work", "help", types /doctor, or when you hit a confusing error yourself (disk full, linker killed, lock file busy, git refusing, command not found).
argument-hint: "[what looks wrong, in their words]"
---

# /doctor

What they said (may be empty): `$ARGUMENTS`

Calm and quick. Work through the checks **in order**, fix what you safely can as you go, and stop early once the
problem is found and fixed (still glance at step 1, it's cheap). Then tell them what happened (step 6). Talk as in
"Talking to humans" in `CLAUDE.md`: no jargon, and a human only ever answers a yes/no, types one `! command` to sign
in, or clicks one link. Work in the project folder (the one with `CLAUDE.md`).

## Safety rules (never bend)
- **Never throw away anyone's work** without a clear "yes" from them: no `git reset --hard`, `git clean`,
  `git checkout -- .`, `git stash drop`, deleting branches with unmerged commits, or deleting files outside the
  build folders. To get changes out of the way, save them: commit them to a new branch (preferred) or `git stash`.
- **Never force-push `main`** and never push straight to it.
- Delete only build output that rebuilds itself: what `cargo tidy` removes, plus `web/pkg`, `web/assets`,
  `docs/teaser/build`. Scratch files you made yourself this session are fine to delete too.
- Don't install or upgrade anything big (a new Rust version, system packages) without a yes/no first; small
  project tools (`rustup target add`, `cargo install wasm-bindgen-cli`) are fine.

## 1. Disk space
```sh
cargo tidy            # says build-file size and free space; cleans up if needed
```
If `cargo` itself is missing, skip to step 2. Disk still nearly full afterwards? Offer `cargo tidy deep` (yes/no:
"I can clear all of the project's build files to free about N GB; the next build will take a few minutes longer.
OK?"). If it is still full after that, the space is used outside the project: say so plainly and suggest they
empty the bin/trash or remove big downloads. Never delete their files yourself.
Also list other side copies: `git worktree list`. Old ones under `.claude/worktrees/` whose work is merged or
pushed can go (`git worktree remove <path>`); ones with unsaved changes stay.

## 2. Tools
Check each; fix what's missing.

| Check | Fix |
|---|---|
| `git --version` | Not installed: ask yes/no, then install with the system's package manager (macOS: `xcode-select --install`; Windows: `winget install --id Git.Git -e`; Linux: the distro's package manager). |
| `gh auth status` | Not installed: install like git (`brew install gh`, `winget install --id GitHub.cli -e`, distro package). Not signed in: ask them to type `! gh auth login` and follow its prompts. |
| `cargo --version`, `rustup show` | No Rust: ask yes/no, then install from <https://rustup.rs> (Windows: `winget install --id Rustlang.Rustup -e`). Build errors that mention a newer Rust edition or "requires rustc 1.xx": `rustup update stable`. If the repo has a `rust-toolchain`/`rust-toolchain.toml` file, `rustup show` installs that version. |
| `rustup target list --installed` has `wasm32-unknown-unknown` | `rustup target add wasm32-unknown-unknown` |
| `wasm-bindgen --version` equals the `wasm-bindgen` version in `Cargo.lock` | `cargo install wasm-bindgen-cli --version <that version> --locked` (needed for `tools/build-web.sh`) |
| Art work only: the painter builds (`cargo build -p painter --profile art --bin crabpaint`) | Read the error; usually one of the rows above. |
| Linux only, build fails linking or mentions `alsa`, `udev`, `wayland`, `xkbcommon`, `pkg-config` | Ask yes/no, then install the system libraries Bevy needs (Debian/Ubuntu: `libudev-dev libwayland-dev libxkbcommon-dev pkg-config`; `libasound2-dev` if audio). |

A build that dies with "Killed", "signal 9" or "out of memory" means the computer ran out of memory: retry with
`cargo build -j 2`.

## 3. The project copy (git)
```sh
git status
git rev-parse --abbrev-ref HEAD
git fetch origin && git rev-list --left-right --count origin/main...HEAD
```
- **Stuck in the middle of something** (`git status` says rebasing, merging, cherry-picking): if it's your own
  unfinished work, finish it (resolve conflicts, keeping both sides' intent). If it's unclear whose it is or what it
  was for, `git rebase --abort` / `git merge --abort` (these keep all committed work) and say so.
- **Unsaved changes** that block what they want: save them on a branch
  (`git switch -c saved/<date>-<short-name> && git add -A && git commit -m "Saved work in progress"`), push it, then
  go back. Tell them their changes are saved and safe.
- **Behind `main`** on `main`: `git pull --ff-only`. If that fails because `main` has local commits, move them to a
  branch (`git branch saved/<date> && git reset --keep origin/main`) and open a pull request for them as usual.
- **Detached HEAD or a weird branch**: put any commits on a branch, then `git switch main`.
- A file that "keeps coming back" or git says is too big: it's probably generated junk; see the "never commit"
  rule in `CLAUDE.md`, remove it from the change (not from their disk) and make sure `.gitignore` covers it.
- `index.lock` exists: only if no git command is running (`ps`/Task Manager show none), delete `.git/index.lock`.

## 4. Network and GitHub
`gh auth status`, then `gh api user --jq .login` and `git ls-remote origin HEAD`. If those fail: the internet or
GitHub is down, or the sign-in expired (ask for `! gh auth login`). No access to the project: they need to accept
the invite at <https://github.com/StevenKauwe/crabscape/invitations>. Check <https://www.githubstatus.com> if
everything else looks right.

## 5. Still broken?
Run the checks from `CLAUDE.md` (`cargo build`, `cargo test`) and read the first error, not the last. Fix it if it's
a real problem in the project (that's a normal task: branch, pull request, merge). If you can't fix it within the
budget, open an issue labelled `bug` with what you found, and tell them "I've left a note so it gets fixed".

## 6. Tell them
One to three plain sentences: what was wrong, what you did, and whether anything is needed from them. For example:
"The disk was nearly full of old build files; I cleared 14 GB and everything works again. The next build will take a
couple of minutes longer." or "Your unsaved changes were in the way, so I saved them somewhere safe and brought your
copy up to date. Nothing lost."
