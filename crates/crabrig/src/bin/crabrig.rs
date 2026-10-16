//! `crabrig`: the player crab's art, from the parametric puppet.
//!
//!     crabrig parts  [--design v0.1|v0.2] [out.svg]               the rig parts file (default art/crab/crab-parts.svg)
//!     crabrig export [--scale S] [--out DIR] [parts.svg]          atlas + crab-parts.json (default into assets/crab/)
//!
//! Run from the repo root: `cargo run -p crabrig --profile art -- <command>`.

use crabrig::crab::{DESIGN, DESIGN_V01, base_pose};

const USAGE: &str = "usage:
  crabrig parts  [--design v0.1|v0.2] [out.svg]
  crabrig export [--scale S] [--out DIR] [parts.svg]";

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
    match cmd.as_str() {
        "parts" => {
            let mut pose = base_pose();
            pose.design = match option(&mut args, "--design").as_deref() {
                None | Some("v0.2") => DESIGN,
                Some("v0.1") => DESIGN_V01,
                Some(v) => return Err(format!("--design v0.1|v0.2, not {v}")),
            };
            let out = args
                .first()
                .cloned()
                .unwrap_or("art/crab/crab-parts.svg".into());
            std::fs::write(&out, crabrig::parts::parts_svg(&pose))
                .map_err(|e| format!("{out}: {e}"))?;
            println!("wrote {out}");
        }
        "export" => {
            let zoom = option(&mut args, "--scale")
                .map(|v| v.parse::<f64>().map_err(|_| format!("--scale: {v}")))
                .transpose()?
                .unwrap_or(1.0);
            let dir = option(&mut args, "--out").unwrap_or("assets/crab".into());
            let source = args
                .first()
                .cloned()
                .unwrap_or("art/crab/crab-parts.svg".into());
            let text = std::fs::read_to_string(&source).map_err(|e| format!("{source}: {e}"))?;
            let e = crabrig::export::export(
                &text,
                &crabrig::export::Options {
                    zoom,
                    source_name: source.clone(),
                },
            )?;
            std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
            std::fs::write(format!("{dir}/crab-atlas.png"), &e.atlas_png)
                .map_err(|e| e.to_string())?;
            std::fs::write(format!("{dir}/crab-parts.json"), &e.json).map_err(|e| e.to_string())?;
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
