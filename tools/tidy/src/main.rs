//! Keeps CrabScape's build files from filling the disk.
//!
//! ```text
//! cargo tidy            say how big the build files are and how much disk is free; clean up if needed
//! cargo tidy report     only say it, never delete anything
//! cargo tidy deep       remove all build files (target/, web/pkg, web/assets, ...) to free the most space
//! cargo tidy hook       what Claude Code runs at session start: quiet unless it cleaned or space is low
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

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

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
        if !lines.is_empty() {
            let text = lines.join(" ");
            // `systemMessage` is shown to the person; `additionalContext` tells Claude what happened.
            println!(
                "{{\"systemMessage\":\"{0}\",\"hookSpecificOutput\":{{\"hookEventName\":\"SessionStart\",\
                 \"additionalContext\":\"Disk housekeeping (cargo tidy) at session start: {0}\"}}}}",
                json_escape(&text)
            );
        }
    } else {
        for l in lines {
            println!("{l}");
        }
    }
}

fn is_crabscape(root: &Path) -> bool {
    fs::read_to_string(root.join("Cargo.toml")).is_ok_and(|t| t.contains("name = \"crabscape\""))
}

/// Does the work and returns what to tell the person (for the hook: only when there is news).
fn run(root: &Path, mode: Mode, max_target: u64, min_free: u64) -> Vec<String> {
    let targets = target_dirs(root);
    let size_of_targets = |t: &[PathBuf]| t.iter().map(|d| tree_size(d)).sum::<u64>();
    let before = size_of_targets(&targets);
    let free_before = free_space(root);
    let low = |free: Option<u64>| free.is_some_and(|f| f < min_free);

    let mut out = Vec::new();
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
