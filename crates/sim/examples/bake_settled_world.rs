//! Offline settled-terrain bake (`docs/design/settled-terrain.md`).
//!
//! `--probe [steps]`  time the whole map for a few steps.
//! `--write [path]`   rebake the chunks whose fingerprint changed in the seed
//!                    at `path` (default: the committed seed); `--jobs N`
//!                    bakes N chunks at once (default 3).
//! `--seams [path]`   bake the whole map as one grid and measure how far the
//!                    chunked seed at `path` differs from it.
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let committed = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/world-settled.postcard.deflate"
    );
    let path = args
        .iter()
        .skip(1)
        .find(|a| !a.starts_with("--") && a.parse::<usize>().is_err())
        .map_or(committed, String::as_str);
    let jobs = args
        .iter()
        .position(|a| a == "--jobs")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok())
        .unwrap_or(3);
    match args.first().map(String::as_str) {
        Some("--write") => crabsim::underwater::seed_bake::bake(std::path::Path::new(path), jobs),
        Some("--seams") => crabsim::underwater::seed_bake::seams(std::path::Path::new(path)),
        Some("--probe") => crabsim::underwater::seed_bake::probe(
            args.get(1).and_then(|n| n.parse().ok()).unwrap_or(30),
        ),
        _ => panic!("use --probe [steps], --write [path] [--jobs N] or --seams [path]"),
    }
}
