//! `crabrig`: the player crab's art (and the creatures'), from the parametric puppets.
//!
//!     crabrig parts  [--creature C] [out.svg]                     the rig parts file (default art/crab/crab-parts.svg)
//!     crabrig export [--creature C] [--scale S] [--out DIR] [parts.svg]
//!                                                                 atlas + <name>-parts.json (default into assets/crab/)
//!
//! `--creature snapping-shrimp`: the creature's parts file is art/creatures/<C>/<C>-parts.svg, its export goes into
//! assets/creatures/.
//!
//! Run from the repo root: `cargo run -p crabrig --profile art -- <command>`.

use crabrig::crab::base_pose;

const USAGE: &str = "usage:
  crabrig parts  [--creature snapping-shrimp] [out.svg]
  crabrig export [--creature snapping-shrimp] [--scale S] [--out DIR] [parts.svg]";

/// The creatures with puppets.
const CREATURES: [&str; 1] = ["snapping-shrimp"];

/// Pull `--flag value` out of the arguments.
fn option(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    let v = args.get(i + 1).cloned();
    args.drain(i..(i + 2).min(args.len()));
    v
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    let cmd = if args.is_empty() {
        String::new()
    } else {
        args.remove(0)
    };
    let creature = option(&mut args, "--creature");
    if let Some(c) = &creature
        && !CREATURES.contains(&c.as_str())
    {
        return Err(format!(
            "no creature {c} (there are: {})",
            CREATURES.join(", ")
        ));
    }
    let (name, default_svg, default_out) = match &creature {
        Some(c) => (
            c.clone(),
            format!("art/creatures/{c}/{c}-parts.svg"),
            "assets/creatures".to_string(),
        ),
        None => (
            "crab".to_string(),
            "art/crab/crab-parts.svg".to_string(),
            "assets/crab".to_string(),
        ),
    };
    match cmd.as_str() {
        "parts" => {
            let out = args.first().cloned().unwrap_or(default_svg);
            let svg = match creature.as_deref() {
                Some(_) => crabrig::shrimp::parts_svg(),
                None => crabrig::parts::parts_svg(&base_pose()),
            };
            std::fs::write(&out, svg).map_err(|e| format!("{out}: {e}"))?;
            println!("wrote {out}");
        }
        "export" => {
            let zoom = option(&mut args, "--scale")
                .map(|v| v.parse::<f64>().map_err(|_| format!("--scale: {v}")))
                .transpose()?
                .unwrap_or(1.0);
            let dir = option(&mut args, "--out").unwrap_or(default_out);
            let source = args.first().cloned().unwrap_or(default_svg);
            let text = std::fs::read_to_string(&source).map_err(|e| format!("{source}: {e}"))?;
            let e = crabrig::export::export(
                &text,
                &crabrig::export::Options {
                    zoom,
                    source_name: source.clone(),
                    name: name.clone(),
                },
            )?;
            std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
            std::fs::write(format!("{dir}/{name}-atlas.png"), &e.atlas_png)
                .map_err(|e| e.to_string())?;
            std::fs::write(format!("{dir}/{name}-parts.json"), &e.json)
                .map_err(|e| e.to_string())?;
            println!(
                "exported {} parts from {} groups into {dir}/ (atlas {}x{})",
                e.parts, e.groups, e.size[0], e.size[1]
            );
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() {
    if let Err(e) = run(std::env::args().skip(1).collect()) {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
