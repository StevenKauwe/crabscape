//! Keeps CrabScape's build files from filling the disk.
//!
//! ```text
//! cargo tidy            say how big the build files are and how much disk is free; clean up if needed
//! cargo tidy report     only say it, never delete anything
//! cargo tidy deep       remove all build files (target/, web/pkg, web/assets, ...) to free the most space
//! cargo tidy hook       what Claude Code runs at session start: quiet unless it cleaned, space is low or
//!                       main's latest CI run failed (asked with `gh`; skipped quietly offline)
//! ```
//! Options: `--max-target-gb N` (default 8), `--min-free-gb N` (default 10), `--project DIR`.
//! (`cargo tidy` is an alias in `.cargo/config.toml` for
//! `cargo run -q --release --manifest-path tools/tidy/Cargo.toml --`.)
//!
//! It only ever deletes things that Cargo or the project's build scripts recreate by themselves: inside the
//! project's `target/` folders (the main one and those of Claude's side copies in `.claude/worktrees/`), and with
//! `deep` also the web and teaser build outputs. Folders a build is using right now are skipped (it checks Cargo's
//! own lock files) and locked while being cleaned, so a build that starts meanwhile waits instead of breaking.
//!
//! The cleanup goes in steps, cheapest to redo first:
//! 1. incremental caches (`target/*/incremental`): only make small rebuilds faster;
//! 2. outdated copies in `deps/`: when a library is rebuilt with different settings, Cargo keeps the old copy too;
//! 3. only if the disk is short of space or the files are huge anyway: everything (like `cargo clean`), after which
//!    the next build takes a few minutes longer.
//!
//! Every run (`report` only says what it would do) also tidies Claude's side copies, so finished work never sits on
//! the disk (each copy's `target/` grows to several GB):
//! - a side copy whose work is merged (its commit is the head of a merged pull request, asked with `gh`) or that has
//!   nothing beyond `origin/main` is removed with `git worktree remove`, once it has no unsaved changes and nobody
//!   has touched it for a while ([`MERGED_IDLE`], [`EMPTY_IDLE`]);
//! - a side copy nobody has touched for [`ABANDONED_IDLE`] keeps its work but loses its build files;
//! - local branches whose commit is the head of a merged pull request, and that no copy has checked out, are deleted.
//!
//! It never removes the copy it runs in, a locked copy, one with unsaved changes or one a build is using.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

const USAGE: &str = "\
cargo tidy            say how big the build files are and how much disk is free; clean up if needed
cargo tidy report     only say it, never delete anything
cargo tidy deep       remove all build files (target/, web/pkg, web/assets, ...) to free the most space
cargo tidy hook       quiet unless it cleaned or space is low (Claude Code runs this at session start)
Options: --max-target-gb N (default 8), --min-free-gb N (default 10),
         --project DIR (another copy of the project; default: the one this tool is in)";
const GB: u64 = 1_000_000_000;
/// Cargo's lock files in each build folder; a build holds them while it runs.
const LOCK_FILES: [&str; 3] = [".cargo-lock", ".cargo-build-lock", ".cargo-artifact-lock"];
/// An outdated copy must be at least this old before step 2 removes it.
const STALE_AGE: Duration = Duration::from_secs(2 * 24 * 3600);
/// A side copy whose pull request is merged goes once it has sat untouched this long (its session may be wrapping up).
const MERGED_IDLE: Duration = Duration::from_secs(3600);
/// A side copy with nothing beyond `origin/main` goes after this long untouched (a session may not have started yet).
const EMPTY_IDLE: Duration = Duration::from_secs(2 * 24 * 3600);
/// A side copy untouched this long keeps its work but loses its build files.
const ABANDONED_IDLE: Duration = Duration::from_secs(7 * 24 * 3600);

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Report,
    Deep,
    Hook,
}

fn main() {
    let mut mode = Mode::Normal;
    let mut max_target = 8 * GB;
    let mut min_free = 10 * GB;
    let mut project = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "report" | "--report" | "--dry-run" => mode = Mode::Report,
            "deep" | "--deep" => mode = Mode::Deep,
            "hook" | "--hook" => mode = Mode::Hook,
            "--max-target-gb" | "--min-free-gb" => {
                let Some(n) = args.next().and_then(|v| v.parse::<f64>().ok()) else {
                    eprintln!("{a} needs a number of gigabytes, for example: {a} 8");
                    std::process::exit(2);
                };
                let bytes = (n * GB as f64) as u64;
                if a == "--max-target-gb" {
                    max_target = bytes;
                } else {
                    min_free = bytes;
                }
            }
            "--project" => project = args.next().map(PathBuf::from),
            "-h" | "--help" | "help" => {
                println!("{USAGE}");
                return;
            }
            other => {
                eprintln!("Unknown option '{other}'. Try: cargo tidy --help");
                std::process::exit(2);
            }
        }
    }

    let root =
        project.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."));
    let root = root.canonicalize().unwrap_or(root);
    if !is_crabscape(&root) {
        if mode != Mode::Hook {
            eprintln!(
                "This doesn't look like the CrabScape project folder: {}",
                root.display()
            );
        }
        return;
    }
    let lines = run(&root, mode, max_target, min_free);
    if mode == Mode::Hook {
        let red = main_ci_failed(&root);
        if !lines.is_empty() || red.is_some() {
            let mut shown = lines.join(" ");
            let mut context = if lines.is_empty() {
                String::new()
            } else {
                format!("Disk housekeeping (cargo tidy) at session start: {shown}")
            };
            if let Some(url) = red {
                shown = format!("{shown} The automatic checks on main are failing: {url}")
                    .trim()
                    .into();
                context = format!(
                    "{context} Main's latest CI run FAILED: {url}. Fixing main comes first: /next-task picks the \
                     open \"main is broken\" issue."
                )
                .trim()
                .into();
            }
            // `systemMessage` is shown to the person; `additionalContext` tells Claude what happened.
            println!(
                "{{\"systemMessage\":\"{}\",\"hookSpecificOutput\":{{\"hookEventName\":\"SessionStart\",\
                 \"additionalContext\":\"{}\"}}}}",
                json_escape(&shown),
                json_escape(&context)
            );
        }
    } else {
        for l in lines {
            println!("{l}");
        }
    }
}

/// Session start: did main's latest finished CI run fail? Asks GitHub with `gh`; returns the run's link. Says
/// nothing (None) when `gh` is missing, signed out, offline or slow.
fn main_ci_failed(root: &Path) -> Option<String> {
    let mut gh = Command::new("gh");
    gh.args([
        "run",
        "list",
        "--branch",
        "main",
        "--workflow",
        "ci.yml",
        "--event",
        "push",
    ])
    .args([
        "--status",
        "completed",
        "--limit",
        "1",
        "--json",
        "conclusion,url",
    ])
    .args(["--jq", r#".[0] | "\(.conclusion) \(.url)""#])
    .current_dir(root);
    let out = output_within(&mut gh, Duration::from_secs(8))?;
    let (conclusion, url) = out.trim().split_once(' ')?;
    (conclusion == "failure").then(|| url.to_string())
}

/// Runs `cmd` and returns what it printed, or None if it is missing, fails or takes longer than `limit` (offline, a
/// network that hangs). Nothing is read from the terminal.
fn output_within(cmd: &mut Command, limit: Duration) -> Option<String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Read while it runs, so a long answer can't fill the pipe and stall it.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        stdout.read_to_string(&mut out).map(|_| out)
    });
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()?.ok()
}

/// A local git command in `dir`; its output, or None if it failed.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, PartialEq)]
struct Worktree {
    path: PathBuf,
    head: String,
    /// The branch's short name; None when detached.
    branch: Option<String>,
    /// Locked (`git worktree lock`) or its folder is gone: never touched.
    keep: bool,
}

fn parse_worktrees(porcelain: &str) -> Vec<Worktree> {
    let mut v = Vec::new();
    for block in porcelain.split("\n\n") {
        let mut wt = Worktree {
            path: PathBuf::new(),
            head: String::new(),
            branch: None,
            keep: false,
        };
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => wt.path = PathBuf::from(value),
                "HEAD" => wt.head = value.into(),
                "branch" => wt.branch = Some(value.trim_start_matches("refs/heads/").into()),
                "locked" | "prunable" => wt.keep = true,
                _ => {}
            }
        }
        if !wt.path.as_os_str().is_empty() {
            v.push(wt);
        }
    }
    v
}

/// Is this one of Claude's side copies (`<project>/.claude/worktrees/<name>`)?
fn is_side_copy(path: &Path) -> bool {
    let parts: Vec<_> = path.components().map(|c| c.as_os_str()).collect();
    parts
        .windows(2)
        .any(|w| w[0] == ".claude" && w[1] == "worktrees")
}

/// The commits that were the head of a merged pull request (asked with `gh`; empty when it can't say).
fn merged_heads(root: &Path) -> HashSet<String> {
    let mut gh = Command::new("gh");
    gh.args(["pr", "list", "--state", "merged", "--limit", "300"])
        .args(["--json", "headRefOid", "--jq", ".[].headRefOid"])
        .current_dir(root);
    output_within(&mut gh, Duration::from_secs(10))
        .map(|out| out.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

/// When a side copy was last touched: its last commit or checkout, staged change, or edited file.
fn last_touched(wt: &Path, changed_files: &str) -> Option<SystemTime> {
    let git_dir = PathBuf::from(git(wt, &["rev-parse", "--absolute-git-dir"])?.trim());
    let mut paths = vec![git_dir.join("index"), git_dir.join("logs").join("HEAD")];
    // `git status --porcelain` lines: two status letters, a space, the path.
    paths.extend(
        changed_files
            .lines()
            .filter_map(|l| l.get(3..))
            .map(|f| wt.join(f.trim_matches('"'))),
    );
    paths
        .iter()
        .filter_map(|p| p.metadata().and_then(|m| m.modified()).ok())
        .max()
}

/// Is a build using any of the copy's build folders right now?
fn building(wt: &Path) -> bool {
    let mut probe = Cleaner::default();
    for p in profile_dirs(&wt.join("target")) {
        probe.with_lock(&p, |_| {});
    }
    probe.busy > 0
}

/// Tidies Claude's side copies and merged local branches (see the top of this file). Returns what to tell the person.
fn tidy_side_copies(root: &Path, mode: Mode, now: SystemTime) -> Vec<String> {
    let act = mode != Mode::Report;
    // Learn which branches GitHub deleted and how far main has moved; quietly skipped offline.
    let mut fetch = Command::new("git");
    fetch
        .args(["fetch", "--prune", "--quiet", "origin"])
        .current_dir(root);
    let _ = output_within(&mut fetch, Duration::from_secs(15));
    let Some(list) = git(root, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let worktrees = parse_worktrees(&list);
    let side: Vec<&Worktree> = worktrees
        .iter()
        .filter(|w| is_side_copy(&w.path) && !w.keep)
        .filter(|w| w.path.canonicalize().is_ok_and(|p| p != root))
        .collect();
    let merged = if side.is_empty() && mode == Mode::Hook {
        HashSet::new()
    } else {
        merged_heads(root)
    };

    let (mut removed, mut removed_bytes, mut cleared, mut cleared_bytes) = (0, 0, 0, 0);
    let mut freed_branches = Vec::new();
    for wt in side {
        let Some(changes) = git(&wt.path, &["status", "--porcelain"]) else {
            continue;
        };
        let Some(idle) = last_touched(&wt.path, &changes).and_then(|t| now.duration_since(t).ok())
        else {
            continue;
        };
        if building(&wt.path) {
            continue;
        }
        let is_merged = merged.contains(&wt.head);
        let empty = git(&wt.path, &["rev-list", "--count", "origin/main..HEAD"])
            .is_some_and(|n| n.trim() == "0");
        let finished = changes.trim().is_empty()
            && ((is_merged && idle > MERGED_IDLE) || (empty && idle > EMPTY_IDLE));
        if finished {
            let size = tree_size(&wt.path);
            let path = wt.path.to_string_lossy();
            if !act || git(root, &["worktree", "remove", &path]).is_some() {
                removed += 1;
                removed_bytes += size;
                if let Some(b) = &wt.branch {
                    freed_branches.push(b.clone());
                }
            }
        } else if idle > ABANDONED_IDLE && wt.path.join("target").is_dir() {
            let mut cleaner = Cleaner::default();
            let size = tree_size(&wt.path.join("target"));
            if act {
                cleaner.remove(&wt.path.join("target"));
            }
            cleared += 1;
            cleared_bytes += if act { cleaner.freed } else { size };
        }
    }

    // Local branches whose work is merged and that no copy has checked out (the ones just freed included).
    let mut branches = 0;
    if let Some(refs) = git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname) %(worktreepath)",
            "refs/heads",
        ],
    ) {
        for line in refs.lines() {
            let mut f = line.split(' ');
            let (Some(name), Some(oid)) = (f.next(), f.next()) else {
                continue;
            };
            let checked_out = f.next().is_some_and(|p| !p.is_empty());
            // In a report the finished copies are still there, holding their branches.
            let would_be_free = !act && freed_branches.iter().any(|b| b == name);
            if name == "main" || !merged.contains(oid) || (checked_out && !would_be_free) {
                continue;
            }
            if !act || git(root, &["branch", "-D", name]).is_some() {
                branches += 1;
            }
        }
    }

    let mut out = Vec::new();
    if removed > 0 {
        out.push(format!(
            "{} {removed} finished side cop{} of the project (work merged or none made), {}.",
            if act { "Removed" } else { "Would remove" },
            if removed == 1 { "y" } else { "ies" },
            human(removed_bytes)
        ));
    }
    if cleared > 0 {
        out.push(format!(
            "{} the build files of {cleared} side cop{} nobody has touched for a week (the work in {} is kept), {}.",
            if act { "Cleared" } else { "Would clear" },
            if cleared == 1 { "y" } else { "ies" },
            if cleared == 1 { "it" } else { "them" },
            human(cleared_bytes)
        ));
    }
    if branches > 0 {
        out.push(format!(
            "{} {branches} local branch{} whose work is merged.",
            if act { "Deleted" } else { "Would delete" },
            if branches == 1 { "" } else { "es" }
        ));
    }
    out
}

fn is_crabscape(root: &Path) -> bool {
    fs::read_to_string(root.join("Cargo.toml")).is_ok_and(|t| t.contains("name = \"crabscape\""))
}

/// Does the work and returns what to tell the person (for the hook: only when there is news).
fn run(root: &Path, mode: Mode, max_target: u64, min_free: u64) -> Vec<String> {
    // Finished side copies go first: removing one frees its build files too.
    let mut out = tidy_side_copies(root, mode, SystemTime::now());
    let targets = target_dirs(root);
    let size_of_targets = |t: &[PathBuf]| t.iter().map(|d| tree_size(d)).sum::<u64>();
    let before = size_of_targets(&targets);
    let free_before = free_space(root);
    let low = |free: Option<u64>| free.is_some_and(|f| f < min_free);

    let worktrees = targets.len() > 1;
    let where_ = if worktrees {
        " (including Claude's side copies of the project)"
    } else {
        ""
    };
    let summary = format!(
        "The project's build files take {}{where_}; the disk has {}.",
        human(before),
        free_before.map_or("an unknown amount of free space".into(), |f| format!(
            "{} free",
            human(f)
        ))
    );

    let needed = mode == Mode::Deep || before > max_target || low(free_before);
    if mode == Mode::Report || !needed {
        if mode != Mode::Hook {
            out.push(summary);
            out.push(if mode == Mode::Report && needed {
                "That's more than it should be: `cargo tidy` would clean up.".into()
            } else {
                "That's fine, nothing to clean.".into()
            });
        }
        return out;
    }

    let mut cleaner = Cleaner::default();
    let profiles: Vec<PathBuf> = targets.iter().flat_map(|t| profile_dirs(t)).collect();
    // Steps 1 and 2: cheap to redo.
    for p in &profiles {
        cleaner.with_lock(p, |c| {
            c.remove_contents(&p.join("incremental"), &[]);
            c.remove_stale_deps(&p.join("deps"));
        });
    }
    // Step 3: everything, only when it is really worth it.
    let mid = size_of_targets(&targets);
    let full = mode == Mode::Deep || (mid > GB && (low(free_space(root)) || mid > 2 * max_target));
    if full {
        for p in &profiles {
            cleaner.with_lock(p, |c| c.remove_contents(p, &LOCK_FILES));
        }
        for t in &targets {
            // Leftovers that aren't build folders (cargo's tmp/, old `cargo doc` output...). Build folders were
            // just handled above, with their locks; the browser test's screenshots are kept (it holds no build output).
            if let Ok(rd) = fs::read_dir(t) {
                for e in rd.flatten() {
                    let path = e.path();
                    let name = e.file_name();
                    let keep =
                        name == "CACHEDIR.TAG" || name == "webtest" || contains_profile(&path);
                    if !keep {
                        cleaner.remove(&path);
                    }
                }
            }
        }
    }
    if mode == Mode::Deep {
        for rel in [
            "web/pkg",
            "web/assets",
            "docs/teaser/build",
            "tools/oil-atlas/target",
        ] {
            cleaner.remove(&root.join(rel));
        }
    }

    let after = size_of_targets(&targets);
    let free_after = free_space(root);
    if mode != Mode::Hook {
        out.push(summary);
    }
    if cleaner.freed > 0 {
        out.push(format!(
            "Cleaned up {} of old build files{} (they're rebuilt automatically when needed); {} free now.",
            human(cleaner.freed),
            if mode == Mode::Hook { format!(", {} -> {}", human(before), human(after)) } else { String::new() },
            free_after.map_or("unknown".into(), human)
        ));
        if full {
            out.push(
                "The next game build will take a few minutes longer than usual while it rebuilds."
                    .into(),
            );
        }
    } else if mode != Mode::Hook {
        out.push("There was nothing that could be cleaned safely.".into());
    }
    if cleaner.busy > 0 {
        out.push(format!(
            "{} build folder{} in use by a running build, so I left {} alone.",
            cleaner.busy,
            if cleaner.busy == 1 { " was" } else { "s were" },
            if cleaner.busy == 1 { "it" } else { "them" }
        ));
    }
    if cleaner.failed > 0 && mode != Mode::Hook {
        out.push(format!(
            "{} files couldn't be removed (probably in use); that's harmless.",
            cleaner.failed
        ));
    }
    if low(free_after) {
        out.push(format!(
            "The disk is {}nearly full ({} free). The rest of the space is used outside this project, so I \
             can't safely free more from here: big downloads, videos or other projects are the usual cause.",
            if cleaner.freed > 0 { "still " } else { "" },
            free_after.map_or("unknown".into(), human)
        ));
    }
    out
}

/// The main `target/` plus those of Claude's side copies (`.claude/worktrees/*/target`).
fn target_dirs(root: &Path) -> Vec<PathBuf> {
    let mut v = vec![root.join("target")];
    if let Ok(rd) = fs::read_dir(root.join(".claude").join("worktrees")) {
        let mut wts: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path().join("target"))
            .filter(|p| p.is_dir())
            .collect();
        wts.sort();
        v.extend(wts);
    }
    v.retain(|p| p.is_dir());
    v
}

/// Cargo's per-profile build folders: `target/<profile>` and `target/<platform>/<profile>`.
fn profile_dirs(target: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    for e in fs::read_dir(target).into_iter().flatten().flatten() {
        let p = e.path();
        if is_profile(&p) {
            v.push(p);
        } else if p.is_dir() && !p.is_symlink() {
            v.extend(
                fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| is_profile(p)),
            );
        }
    }
    v
}

fn is_profile(p: &Path) -> bool {
    !p.is_symlink() && p.join(".cargo-lock").is_file()
}

fn contains_profile(p: &Path) -> bool {
    is_profile(p)
        || fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| is_profile(&e.path()))
}

#[derive(Default)]
struct Cleaner {
    freed: u64,
    failed: u64,
    busy: u64,
}

impl Cleaner {
    /// Runs `f` while holding Cargo's locks on the build folder `dir`; skips it if a build has them.
    fn with_lock(&mut self, dir: &Path, f: impl FnOnce(&mut Self)) {
        let mut held = Vec::new();
        for name in LOCK_FILES {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            let Ok(file) = File::options().read(true).write(true).open(&path) else {
                continue;
            };
            match file.try_lock() {
                Ok(()) => held.push(file),
                Err(_) => {
                    self.busy += 1;
                    return;
                }
            }
        }
        f(self);
        drop(held);
    }

    /// Removes a file or folder (never following links out of it).
    fn remove(&mut self, path: &Path) {
        let Ok(meta) = path.symlink_metadata() else {
            return;
        };
        if meta.is_dir() {
            self.remove_contents(path, &[]);
            let _ = fs::remove_dir(path);
        } else {
            match fs::remove_file(path) {
                // A hard-linked file only frees its space when its last name goes.
                Ok(()) => self.freed += if links(&meta) > 1 { 0 } else { meta.len() },
                Err(_) => self.failed += 1,
            }
        }
    }

    fn remove_contents(&mut self, dir: &Path, keep: &[&str]) {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            if !keep.iter().any(|k| e.file_name() == *k) {
                self.remove(&e.path());
            }
        }
    }

    /// In `deps/`, every build of a library is `<name>-<16 hex digits>.<ext>`. When settings change (features,
    /// profile, compiler version) Cargo makes a new one and keeps the old. Remove copies that are older than
    /// [`STALE_AGE`] while a newer copy of the same name exists. If one was still needed, Cargo notices it's missing
    /// and rebuilds just that library.
    fn remove_stale_deps(&mut self, deps: &Path) {
        // name -> hash -> (newest modification time, files)
        let mut groups: HashMap<String, HashMap<String, (SystemTime, Vec<PathBuf>)>> =
            HashMap::new();
        for e in fs::read_dir(deps).into_iter().flatten().flatten() {
            let file_name = e.file_name().to_string_lossy().into_owned();
            let Some((name, hash)) = split_hash(&file_name) else {
                continue;
            };
            let Ok(modified) = e.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            let entry = groups
                .entry(name.to_owned())
                .or_default()
                .entry(hash.to_owned())
                .or_insert((SystemTime::UNIX_EPOCH, Vec::new()));
            entry.0 = entry.0.max(modified);
            entry.1.push(e.path());
        }
        let now = SystemTime::now();
        for copies in groups.into_values() {
            let Some(newest) = copies.values().map(|c| c.0).max() else {
                continue;
            };
            for (modified, files) in copies.into_values() {
                let old = now
                    .duration_since(modified)
                    .is_ok_and(|age| age > STALE_AGE);
                if modified < newest && old {
                    for f in files {
                        self.remove(&f);
                    }
                }
            }
        }
    }
}

/// `libbevy_ui-a3e43bc975618c32.rlib` -> (`libbevy_ui`, `a3e43bc975618c32`).
fn split_hash(file_name: &str) -> Option<(&str, &str)> {
    let stem = file_name.split('.').next()?;
    let (name, hash) = stem.rsplit_once('-')?;
    (hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then_some((name, hash))
}

/// Total size of the files under `path`, not following links (and counting hard-linked files once).
fn tree_size(path: &Path) -> u64 {
    let mut seen = std::collections::HashSet::new();
    let mut total = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let Ok(meta) = p.symlink_metadata() else {
            continue;
        };
        if meta.is_dir() {
            stack.extend(
                fs::read_dir(&p)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path()),
            );
        } else if meta.is_file() && first_link(&meta, &mut seen) {
            total += meta.len();
        }
    }
    total
}

#[cfg(unix)]
fn first_link(meta: &fs::Metadata, seen: &mut std::collections::HashSet<(u64, u64)>) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() < 2 || seen.insert((meta.dev(), meta.ino()))
}

#[cfg(unix)]
fn links(meta: &fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::nlink(meta)
}

#[cfg(not(unix))]
fn links(_: &fs::Metadata) -> u64 {
    1
}

#[cfg(not(unix))]
fn first_link(_: &fs::Metadata, _: &mut std::collections::HashSet<(u64, u64)>) -> bool {
    true
}

/// Free space on the disk holding `path`, in bytes.
#[cfg(unix)]
fn free_space(path: &Path) -> Option<u64> {
    // `df -P` (POSIX format) works the same on Linux and macOS: the 4th column is the free space in 1 KiB blocks.
    let out = std::process::Command::new("df")
        .arg("-Pk")
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let kib: u64 = text
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    Some(kib * 1024)
}

#[cfg(windows)]
fn free_space(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            dir: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` is a NUL-terminated UTF-16 string and the three outputs are valid `u64`s.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, &mut total_free) };
    (ok != 0).then_some(free)
}

#[cfg(not(any(unix, windows)))]
fn free_space(_: &Path) -> Option<u64> {
    None
}

/// 24.1 GB, 809 GB, 350 MB.
fn human(bytes: u64) -> String {
    let gb = bytes as f64 / GB as f64;
    if gb >= 100.0 {
        format!("{gb:.0} GB")
    } else if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", bytes as f64 / 1e6)
    }
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_cargo_file_names() {
        assert_eq!(
            split_hash("libbevy_ui-a3e43bc975618c32.rlib"),
            Some(("libbevy_ui", "a3e43bc975618c32"))
        );
        assert_eq!(
            split_hash("crabscape-fa44e076bc78044f"),
            Some(("crabscape", "fa44e076bc78044f"))
        );
        assert_eq!(
            split_hash("wasm-bindgen-0e63678eb16b4039.d"),
            Some(("wasm-bindgen", "0e63678eb16b4039"))
        );
        assert_eq!(split_hash("crabscape.d"), None);
        assert_eq!(split_hash("libfoo-xyz.rlib"), None);
    }

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(human(24_100_000_000), "24.1 GB");
        assert_eq!(human(809_000_000_000), "809 GB");
        assert_eq!(human(350_000_000), "350 MB");
    }

    #[test]
    fn escapes_json() {
        assert_eq!(json_escape("a \"b\" \\ c\n"), "a \\\"b\\\" \\\\ c\\u000a");
    }

    #[test]
    fn reads_the_worktree_list() {
        let list = "worktree /p\nHEAD aaa\nbranch refs/heads/main\n\n\
                    worktree /p/.claude/worktrees/x\nHEAD bbb\ndetached\n\n\
                    worktree /p/.claude/worktrees/y\nHEAD ccc\nbranch refs/heads/issue-1-y\nlocked\n";
        let w = parse_worktrees(list);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].branch.as_deref(), Some("main"));
        assert_eq!(
            (w[1].head.as_str(), &w[1].branch, w[1].keep),
            ("bbb", &None, false)
        );
        assert_eq!(w[2].branch.as_deref(), Some("issue-1-y"));
        assert!(w[2].keep);
        assert!(!is_side_copy(&w[0].path));
        assert!(is_side_copy(&w[1].path));
        assert!(!is_side_copy(Path::new("/p/worktrees/.claude/x")));
    }

    /// A real project copy with three side copies: one finished (nothing beyond main), one with unsaved work and one
    /// with unpushed commits. Only the finished one goes, and only once it has sat untouched long enough.
    #[test]
    fn removes_only_finished_side_copies() {
        let dir = std::env::temp_dir().join(format!("tidy-wt-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let run = |cwd: &Path, args: &[&str]| {
            let ok = Command::new("git")
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "init.defaultBranch=main",
                ])
                .args(args)
                .current_dir(cwd)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        run(&dir, &["init", "-q", "--bare", "origin.git"]);
        run(&dir, &["clone", "-q", "origin.git", "p"]);
        let root = dir.join("p").canonicalize().unwrap();
        // Build files are ignored, as in the real project.
        fs::write(root.join(".gitignore"), "/target/\n/.claude/worktrees/\n").unwrap();
        run(&root, &["add", ".gitignore"]);
        run(&root, &["commit", "-q", "-m", "start"]);
        run(&root, &["push", "-q", "origin", "main"]);
        for name in ["done", "dirty", "ahead"] {
            run(
                &root,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    name,
                    &format!(".claude/worktrees/{name}"),
                ],
            );
        }
        let wt = root.join(".claude").join("worktrees");
        fs::write(wt.join("dirty").join("notes.txt"), "unsaved").unwrap();
        run(
            &wt.join("ahead"),
            &["commit", "-q", "--allow-empty", "-m", "mine"],
        );
        fs::create_dir_all(wt.join("done").join("target").join("debug")).unwrap();
        fs::write(
            wt.join("done").join("target").join("debug").join("big"),
            vec![0u8; 5000],
        )
        .unwrap();

        // Just touched: nothing goes.
        assert!(tidy_side_copies(&root, Mode::Normal, SystemTime::now()).is_empty());
        // Three days later: a report only says what it would do.
        let later = SystemTime::now() + Duration::from_secs(3 * 24 * 3600);
        let said = tidy_side_copies(&root, Mode::Report, later);
        assert!(
            said[0].starts_with("Would remove 1 finished side copy"),
            "{said:?}"
        );
        assert!(wt.join("done").exists());
        // Then for real.
        let said = tidy_side_copies(&root, Mode::Normal, later);
        assert!(
            said[0].starts_with("Removed 1 finished side copy"),
            "{said:?}"
        );
        assert!(!wt.join("done").exists());
        assert!(wt.join("dirty").join("notes.txt").exists());
        assert!(wt.join("ahead").exists());
        // A week and more on, the copies with work keep it but lose their build files.
        fs::create_dir_all(wt.join("ahead").join("target").join("debug")).unwrap();
        let much_later = SystemTime::now() + Duration::from_secs(8 * 24 * 3600);
        let said = tidy_side_copies(&root, Mode::Normal, much_later);
        assert!(
            said[0].starts_with("Cleared the build files of 1 side copy"),
            "{said:?}"
        );
        assert!(!wt.join("ahead").join("target").exists());
        assert!(wt.join("ahead").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn cleans_only_outdated_copies_and_caches() {
        let dir = std::env::temp_dir().join(format!("tidy-test-{}", std::process::id()));
        let profile = dir.join("target").join("debug");
        let deps = profile.join("deps");
        fs::create_dir_all(&deps).unwrap();
        fs::create_dir_all(profile.join("incremental").join("crabscape-1")).unwrap();
        fs::write(profile.join(".cargo-lock"), "").unwrap();
        fs::write(
            profile.join("incremental").join("crabscape-1").join("x"),
            vec![0u8; 1000],
        )
        .unwrap();
        let old = SystemTime::now() - Duration::from_secs(10 * 24 * 3600);
        let write = |name: &str, time: SystemTime| {
            let f = File::create(deps.join(name)).unwrap();
            f.set_len(100).unwrap();
            f.set_modified(time).unwrap();
        };
        write("libbevy-0000000000000001.rlib", old); // outdated: a newer copy exists
        write("libbevy-0000000000000001.rmeta", old);
        write("libbevy-0000000000000002.rlib", SystemTime::now());
        write("libserde-0000000000000003.rlib", old); // old but the only copy: kept

        let mut c = Cleaner::default();
        c.with_lock(&profile, |c| {
            c.remove_contents(&profile.join("incremental"), &[]);
            c.remove_stale_deps(&deps);
        });
        assert_eq!(c.busy, 0);
        assert_eq!(c.freed, 1000 + 200);
        assert!(deps.join("libbevy-0000000000000002.rlib").exists());
        assert!(deps.join("libserde-0000000000000003.rlib").exists());
        assert!(!deps.join("libbevy-0000000000000001.rlib").exists());
        assert!(profile.join("incremental").is_dir());
        assert!(profile.join(".cargo-lock").exists());
        assert_eq!(profile_dirs(&dir.join("target")), vec![profile.clone()]);

        // A running build holds the lock: hands off.
        let lock = File::options()
            .write(true)
            .open(profile.join(".cargo-lock"))
            .unwrap();
        lock.lock().unwrap();
        let mut c = Cleaner::default();
        c.with_lock(&profile, |c| c.remove_contents(&deps, &[]));
        assert_eq!((c.busy, c.freed), (1, 0));
        drop(lock);
        fs::remove_dir_all(&dir).unwrap();
    }
}
