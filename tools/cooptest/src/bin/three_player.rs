//! Manual entry point for the same three-player scenario run by co-op CI.
#[allow(dead_code)]
#[path = "../chrome.rs"]
mod chrome;
#[path = "../three_player.rs"]
mod three_player;
use chrome::{Options, Result};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(args.len() == 2 || (args.len() == 3 && args[2] == "--split")) || args[0] != "--url" {
        return Err("usage: three_player --url <cellular preview URL> [--split]".into());
    }
    let dir = std::env::temp_dir().join(format!("crab-three-player-{}", std::process::id()));
    three_player::run(
        &args[1],
        &dir,
        Options {
            fake_mic: false,
            slow: 1.0,
        },
        args.len() == 3,
        3,
    )
}
