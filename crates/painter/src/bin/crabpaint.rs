//! `crabpaint`: the oil painter on the command line.
//!
//!     crabpaint oilphoto <in.png|jpg> <out.png> [fx fy reach] [--nocturne G] [--attention on|off] [--key K] [--ground ochre|subject]
//!     crabpaint oilmix <out.png>
//!
//! `fx fy reach`: where the painter looks, as fractions of the picture (default 0.5 0.4 0.34). The flags default to
//! the environment variables the painter has always read (`OIL_NOCTURNE`, `OIL_ATTENTION`, `OIL_KEY`,
//! `OIL_GROUND`), so `OIL_ATTENTION=off crabpaint oilphoto ...` and `crabpaint oilphoto ... --attention off` are the
//! same. Run it optimised: `cargo run -p painter --profile art --bin crabpaint -- ...`.

use painter::PhotoOptions;

const USAGE: &str = "usage:
  crabpaint oilphoto <in> <out.png> [fx fy reach] [--nocturne G] [--attention on|off] [--key K] [--ground ochre|subject]
  crabpaint oilmix <out.png>";

fn photo(args: &[String]) -> Result<(), String> {
    let mut opts = PhotoOptions::from_env();
    let mut plain = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        let num = |v: String, name: &str| v.parse::<f32>().map_err(|_| format!("{name}: not a number: {v}"));
        match a.as_str() {
            "--nocturne" => opts.nocturne = Some(num(value(a)?, a)?),
            "--key" => opts.key = num(value(a)?, a)?,
            "--attention" => {
                opts.attention = match value(a)?.as_str() {
                    "on" => true,
                    "off" => false,
                    v => return Err(format!("--attention on|off, not {v}")),
                }
            }
            "--ground" => {
                opts.ground_subject = match value(a)?.as_str() {
                    "subject" => true,
                    "ochre" => false,
                    v => return Err(format!("--ground ochre|subject, not {v}")),
                }
            }
            f if f.starts_with("--") => return Err(format!("unknown flag {f}\n{USAGE}")),
            _ => plain.push(a.clone()),
        }
    }
    painter::run_photo_with(&plain, &opts)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("oilphoto") if args.len() >= 3 => photo(&args[1..]),
        Some("oilmix") if args.len() == 2 => painter::run_mix(&args[1..]),
        _ => Err(USAGE.to_string()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(2);
    }
}
