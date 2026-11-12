//! How big each animal is drawn, measured from its art: the opaque pixels of its parts at rest, in world units
//! (1 cm = 6.25), set against its research size and its hurtbox on a like-for-like measurement
//! (`docs/design/creatures/README.md`, "Sizes at crab scale").

use crate::puppet::PartsFile;
use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};

/// World units per centimetre.
pub const CM: f32 = 6.25;

/// The drawn extent of `parts` (all parts when empty), in world units: (width, height), from the opaque pixels of
/// each part's picture placed where the art file drew it, at `scale` world units per art unit.
pub fn drawn(json: &str, atlas_png: &[u8], scale: f32, parts: &[&str]) -> (f32, f32) {
    let file: PartsFile = serde_json::from_str(json).expect("a parts file is valid");
    let image = Image::from_buffer(
        atlas_png,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .expect("an atlas is a png");
    let w = image.width() as usize;
    let data = image.data.as_ref().expect("decoded pixels");
    let mut lo = [f32::INFINITY; 2];
    let mut hi = [f32::NEG_INFINITY; 2];
    for p in file
        .parts
        .iter()
        .filter(|p| parts.is_empty() || parts.contains(&p.name.as_str()))
    {
        let [rx, ry, rw, rh] = p.rect.map(|v| v as usize);
        for j in 0..rh {
            for i in 0..rw {
                if data[((ry + j) * w + rx + i) * 4 + 3] < 128 {
                    continue;
                }
                // The pixel's middle, in art units.
                let x = p.bbox[0] + (i as f32 + 0.5) / rw as f32 * p.bbox[2];
                let y = p.bbox[1] + (j as f32 + 0.5) / rh as f32 * p.bbox[3];
                lo = [lo[0].min(x), lo[1].min(y)];
                hi = [hi[0].max(x), hi[1].max(y)];
            }
        }
    }
    assert!(lo[0].is_finite(), "no opaque pixels in {parts:?}");
    ((hi[0] - lo[0]) * scale, (hi[1] - lo[1]) * scale)
}

/// One animal's size, three ways, on one measurement.
struct Row {
    animal: &'static str,
    /// What is measured (the research's own measure).
    measure: &'static str,
    /// The research range for it (cm) and where it comes from.
    real: (f32, f32),
    source: &'static str,
    /// Drawn, from the art (world units).
    drawn: f32,
    /// The hurtbox along the same measure (world units), if it covers it.
    hit: Option<f32>,
}

/// The art: a parts file and its ink atlas.
macro_rules! art {
    ($n:literal) => {
        (
            include_str!(concat!("../assets/creatures/", $n, "-parts.json")),
            &include_bytes!(concat!("../assets/creatures/", $n, "-atlas.png"))[..],
        )
    };
}

/// World units per art unit for a drawing fitted to `length` end to end (`CreatureArt::load`).
fn fitted(json: &str, length: f32) -> f32 {
    let f: PartsFile = serde_json::from_str(json).expect("a parts file is valid");
    let x0 = f
        .parts
        .iter()
        .map(|p| p.bbox[0])
        .fold(f32::INFINITY, f32::min);
    let x1 = f
        .parts
        .iter()
        .map(|p| p.bbox[0] + p.bbox[2])
        .fold(f32::NEG_INFINITY, f32::max);
    length / (x1 - x0)
}

fn rows() -> Vec<Row> {
    use crabsim::{Kind, Tuning};
    let t = Tuning::default();
    let body = |k: Kind| k.tuning(&t).body;
    let w =
        |(json, png): (&str, &[u8]), scale: f32, parts: &[&str]| drawn(json, png, scale, parts).0;
    let h =
        |(json, png): (&str, &[u8]), scale: f32, parts: &[&str]| drawn(json, png, scale, parts).1;
    let crab = (
        include_str!("../assets/crab/crab-parts.json"),
        &include_bytes!("../assets/crab/crab-atlas.png")[..],
    );
    let shrimp = art!("snapping-shrimp");
    let mantis = art!("mantis-shrimp");
    let moray = art!("snowflake-moray");
    // The moray as the game's sea has it (`crabsim::tuning::SEA_MORAY_SCALE`).
    let sea = crabsim::tuning::SEA_MORAY_SCALE;
    let moray_box = (body(Kind::Moray).0 * sea, body(Kind::Moray).1 * sea);
    let moray_scale = crate::critters::MORAY_ART_SCALE * moray_box.0 / 44.0;
    vec![
        Row {
            animal: "Our crab (blue-crab-like)",
            measure: "carapace width, spine tip to spine tip",
            real: (11.3, 13.0),
            source: "females mature at 11.3-13.0 cm [HART21]",
            drawn: w(crab, crate::puppet::ART_SCALE, &["carapace"]),
            // The box is the shell between the lateral spines.
            hit: Some(2.0 * t.body_half_width),
        },
        Row {
            animal: "Snapping shrimp",
            measure: "total length",
            real: (3.0, 5.5),
            source: "up to 5.5 cm [HS98], generally 3-5 [ALPH]",
            drawn: w(shrimp, fitted(shrimp.0, body(Kind::Snapper).0), &[]),
            hit: Some(body(Kind::Snapper).0),
        },
        Row {
            animal: "Peacock mantis shrimp",
            measure: "total length",
            real: (3.0, 18.0),
            source: "3-18 cm [ADW]",
            drawn: w(mantis, fitted(mantis.0, body(Kind::Mantis).0), &[]),
            hit: Some(body(Kind::Mantis).0),
        },
        Row {
            animal: "Snowflake moray",
            measure: "total length (swimming out)",
            real: (50.0, 100.0),
            source: "common 50, max 100 cm [FB-EN]",
            // Strung by `critters.rs` from its head and segments (world units at the head box's scale).
            drawn: moray_box.0 / 44.0
                * (crate::critters::MORAY_HEAD_HALF
                    + crate::critters::moray_link_at(crate::critters::NECK - 1)
                    + crate::critters::MORAY_SEGMENT_HALF),
            // Only its head is hit: what comes out of the crevice.
            hit: None,
        },
        Row {
            animal: "Snowflake moray",
            measure: "head length (its hurtbox)",
            real: (10.0, 14.0),
            source: "not verified (an eighth to a seventh of its length)",
            drawn: w(moray, moray_scale, &["head"]),
            hit: Some(moray_box.0),
        },
        Row {
            animal: "Snowflake moray",
            measure: "body depth at the gill opening",
            real: (100.0 / 23.0, 100.0 / 14.0),
            source: "1/14-1/23 of its length [FB-EN-M]",
            drawn: h(moray, moray_scale, &["head"]),
            hit: Some(moray_box.1),
        },
        Row {
            animal: "Queen triggerfish",
            measure: "total length",
            real: (30.0, 60.0),
            source: "common 30, max 60 cm [FB-BV]",
            drawn: w(
                art!("queen-triggerfish"),
                crate::triggerfish::ART_SCALE,
                &[],
            ),
            hit: Some(body(Kind::Triggerfish).0),
        },
        Row {
            animal: "Common octopus",
            measure: "mantle and head length (arms extra)",
            real: (15.0, 25.0),
            source: "mantle to 25 cm (Wikipedia)",
            drawn: w(
                art!("common-octopus"),
                crate::octopus::ART_SCALE,
                &["mantle", "head"],
            ),
            hit: Some(body(Kind::Octopus).0),
        },
        Row {
            animal: "Alarm jelly (Atolla)",
            measure: "bell diameter",
            real: (2.0, 17.0),
            source: "A. wyvillei 2-17 cm [WP-AW]",
            drawn: w(
                art!("alarm-jelly"),
                crate::alarm_jelly::ART_SCALE,
                &["bell"],
            ),
            hit: Some(body(Kind::AlarmJelly).0),
        },
        Row {
            animal: "Loggerhead turtle",
            measure: "straight carapace length",
            real: (85.0, 100.0),
            source: "average 85-100 cm [ADW]",
            drawn: w(
                art!("loggerhead-turtle"),
                crate::creatures::turtle::ART_SCALE,
                &["body"],
            ),
            hit: None,
        },
        Row {
            animal: "Loggerhead turtle",
            measure: "carapace and head (its hurtbox)",
            real: (90.0, 115.0),
            source: "the carapace above plus the head (not verified)",
            drawn: w(
                art!("loggerhead-turtle"),
                crate::creatures::turtle::ART_SCALE,
                &["body", "head", "eye"],
            ),
            hit: Some(body(Kind::LoggerheadTurtle).0),
        },
        Row {
            animal: "Humboldt squid",
            measure: "total length, fin to arm tips (tentacles extra)",
            real: (50.0, 80.0),
            source: "common 50-80 cm TL [SLB]",
            drawn: w(
                art!("humboldt-squid"),
                crate::creatures::squid::ART_SCALE,
                &["fin", "body", "head", "arms-upper", "arms-lower"],
            ),
            hit: None,
        },
        Row {
            animal: "Humboldt squid",
            measure: "mantle and head (its hurtbox)",
            real: (40.0, 75.0),
            source: "the above without the arms (not verified)",
            drawn: w(
                art!("humboldt-squid"),
                crate::creatures::squid::ART_SCALE,
                &["fin", "body", "head"],
            ),
            hit: Some(body(Kind::HumboldtSquid).0),
        },
        Row {
            animal: "Viperfish",
            measure: "standard length (tail fin extra)",
            real: (20.0, 35.0),
            source: "max 35 cm SL [FB-CS]",
            drawn: w(
                art!("viperfish"),
                crate::creatures::viperfish::ART_SCALE,
                &["body", "head", "jaw"],
            ),
            hit: Some(body(Kind::Viperfish).0),
        },
        Row {
            animal: "Goliath grouper",
            measure: "total length",
            real: (150.0, 250.0),
            source: "common 150, max 250 cm TL [FB]",
            drawn: w(
                art!("goliath-grouper"),
                crate::creatures::grouper::ART_SCALE,
                &[],
            ),
            hit: Some(body(Kind::GoliathGrouper).0),
        },
        Row {
            animal: "American lobster",
            measure: "body length, claws and feelers extra",
            real: (20.0, 61.0),
            source: "commonly 20-61 cm [WP]; about 25 cm caught [ADW]",
            drawn: w(
                art!("american-lobster"),
                crate::creatures::lobster::ART_SCALE,
                &["tailfan", "abdomen-a", "abdomen-b", "carapace"],
            ),
            hit: Some(body(Kind::Lobster).0),
        },
        Row {
            animal: "Red hermit crab",
            measure: "length, shell and the crab out of it",
            real: (10.0, 12.0),
            source: "about 10, up to 12 cm (Wikipedia; Oldenburg Giglio guide)",
            drawn: w(
                art!("hermit-crab"),
                crate::creatures::hermit::ART_SCALE,
                &[
                    "shell",
                    "body",
                    "head",
                    "eye-a",
                    "eye-b",
                    "claw-small",
                    "claw-big",
                    "claw-big-finger",
                    "leg-a",
                    "leg-b",
                ],
            ),
            hit: None,
        },
        Row {
            animal: "Red hermit crab",
            measure: "shell and head (its hurtbox)",
            real: (6.0, 10.0),
            source: "the above without the claws and legs (not verified)",
            drawn: w(
                art!("hermit-crab"),
                crate::creatures::hermit::ART_SCALE,
                &["shell", "body", "head"],
            ),
            hit: Some(body(Kind::HermitCrab).0),
        },
        Row {
            animal: "Bobbit worm",
            measure: "body width",
            real: (1.0, 2.55),
            source: "the 299 cm record was 25.5 mm wide [WP-EA]",
            drawn: h(
                art!("bobbit-worm"),
                crate::creatures::bobbit::ART_SCALE,
                &["seg-a"],
            ),
            hit: Some(body(Kind::BobbitWorm).0),
        },
        Row {
            animal: "Giant isopod",
            measure: "body length, feelers extra",
            real: (19.0, 36.0),
            source: "typically 19-36 cm [WIKI-GI]",
            drawn: w(
                art!("giant-isopod"),
                crate::creatures::isopod::ART_SCALE,
                &["head", "body", "rear-body", "pleotelson"],
            ),
            hit: Some(body(Kind::GiantIsopod).0),
        },
        Row {
            animal: "Pacific hagfish (a knot of six)",
            measure: "the knot's width (each fish 30-63.5 cm, coiled)",
            real: (15.0, 35.0),
            source: "fish 30-63.5 cm [ADW]; the knot's size not verified",
            drawn: w(art!("hagfish"), crate::creatures::hagfish::ART_SCALE, &[]),
            hit: Some(body(Kind::Hagfish).0),
        },
        Row {
            animal: "Bluntnose sixgill",
            measure: "total length",
            real: (65.0, 480.0),
            source: "up to 4.8 m [FLMNH-SG]; the 65 cm low end not verified; a 2 m one, a design call",
            // Drawn procedurally to its sim body (`src/deep.rs`).
            drawn: body(Kind::Sixgill).0,
            hit: Some(body(Kind::Sixgill).0),
        },
        Row {
            animal: "Scavenging amphipod",
            measure: "one animal's length",
            real: (1.5, 2.5),
            source: "2 cm, a game choice (sizes not verified)",
            drawn: w(
                art!("amphipod-swarm"),
                crate::creatures::amphipods::ART_SCALE,
                &["body", "legs"],
            ),
            hit: None,
        },
    ]
}

/// The README's size table, from the art: `cargo test --bin crabscape sizes -- --nocapture`.
#[test]
fn every_animal_is_drawn_and_hit_at_its_real_size() {
    println!("| Animal | Measure | Real (cm) | Drawn | Hurtbox |");
    println!("|---|---|---|---|---|");
    let mut wrong = Vec::new();
    for r in rows() {
        let cm = r.drawn / CM;
        let hit = r
            .hit
            .map_or("-".to_string(), |b| format!("{:.1} cm ({b:.0})", b / CM));
        println!(
            "| {} | {} | {:.1}-{:.1}: {} | {cm:.1} cm ({:.0}) | {hit} |",
            r.animal, r.measure, r.real.0, r.real.1, r.source, r.drawn
        );
        // Drawn within the research range (5% for the eye's slack on an edge).
        if !(r.real.0 * 0.95..=r.real.1 * 1.05).contains(&cm) {
            wrong.push(format!(
                "{} {}: drawn {cm:.1} cm, real {:?}",
                r.animal, r.measure, r.real
            ));
        }
        // The hurtbox fits the drawing it covers: a little inside the silhouette, never past it by much.
        if let Some(b) = r.hit
            && !(0.75..=1.1).contains(&(b / r.drawn))
        {
            wrong.push(format!(
                "{} {}: hurtbox {b:.0} for a drawing {:.0}",
                r.animal, r.measure, r.drawn
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The swimming moray's head and segments are what the art draws, and the segments overlap (no gaps).
#[test]
fn the_moray_is_drawn_whole() {
    let (json, png) = art!("snowflake-moray");
    let s = crate::critters::MORAY_ART_SCALE;
    let head = drawn(json, png, s, &["head"]).0;
    let segment = drawn(json, png, s, &["segment-a"]).0;
    assert!(
        (head / 2.0 - crate::critters::MORAY_HEAD_HALF).abs() < 1.5,
        "head {head}"
    );
    assert!(
        (segment / 2.0 - crate::critters::MORAY_SEGMENT_HALF).abs() < 1.5,
        "segment {segment}"
    );
    assert!(
        crate::critters::MORAY_LINK < segment,
        "gaps between segments"
    );
}

#[cfg(test)]
/// A 5 x 7 letter (rows top to bottom, `#` lit), for the lineup's labels.
fn glyph(c: char) -> [&'static str; 7] {
    match c {
        'A' => [
            " ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'B' => [
            "#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### ",
        ],
        'C' => [
            " ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### ",
        ],
        'D' => [
            "#### ", "#   #", "#   #", "#   #", "#   #", "#   #", "#### ",
        ],
        'E' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####",
        ],
        'F' => [
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#    ",
        ],
        'G' => [
            " ### ", "#   #", "#    ", "# ###", "#   #", "#   #", " ####",
        ],
        'H' => [
            "#   #", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'I' => [
            " ### ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        'J' => [
            "  ###", "   # ", "   # ", "   # ", "   # ", "#  # ", " ##  ",
        ],
        'K' => [
            "#   #", "#  # ", "# #  ", "##   ", "# #  ", "#  # ", "#   #",
        ],
        'L' => [
            "#    ", "#    ", "#    ", "#    ", "#    ", "#    ", "#####",
        ],
        'M' => [
            "#   #", "## ##", "# # #", "# # #", "#   #", "#   #", "#   #",
        ],
        'N' => [
            "#   #", "#   #", "##  #", "# # #", "#  ##", "#   #", "#   #",
        ],
        'O' => [
            " ### ", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'P' => [
            "#### ", "#   #", "#   #", "#### ", "#    ", "#    ", "#    ",
        ],
        'Q' => [
            " ### ", "#   #", "#   #", "#   #", "# # #", "#  # ", " ## #",
        ],
        'R' => [
            "#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #",
        ],
        'S' => [
            " ####", "#    ", "#    ", " ### ", "    #", "    #", "#### ",
        ],
        'T' => [
            "#####", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'U' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", "#   #", " ### ",
        ],
        'V' => [
            "#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  ",
        ],
        'W' => [
            "#   #", "#   #", "#   #", "# # #", "# # #", "# # #", " # # ",
        ],
        'X' => [
            "#   #", "#   #", " # # ", "  #  ", " # # ", "#   #", "#   #",
        ],
        'Y' => [
            "#   #", "#   #", " # # ", "  #  ", "  #  ", "  #  ", "  #  ",
        ],
        'Z' => [
            "#####", "    #", "   # ", "  #  ", " #   ", "#    ", "#####",
        ],
        '0' => [
            " ### ", "#   #", "#  ##", "# # #", "##  #", "#   #", " ### ",
        ],
        '1' => [
            "  #  ", " ##  ", "  #  ", "  #  ", "  #  ", "  #  ", " ### ",
        ],
        '2' => [
            " ### ", "#   #", "    #", "   # ", "  #  ", " #   ", "#####",
        ],
        '3' => [
            "#####", "   # ", "  #  ", "   # ", "    #", "#   #", " ### ",
        ],
        '4' => [
            "   # ", "  ## ", " # # ", "#  # ", "#####", "   # ", "   # ",
        ],
        '5' => [
            "#####", "#    ", "#### ", "    #", "    #", "#   #", " ### ",
        ],
        '6' => [
            "  ## ", " #   ", "#    ", "#### ", "#   #", "#   #", " ### ",
        ],
        '7' => [
            "#####", "    #", "   # ", "  #  ", " #   ", " #   ", " #   ",
        ],
        '8' => [
            " ### ", "#   #", "#   #", " ### ", "#   #", "#   #", " ### ",
        ],
        '9' => [
            " ### ", "#   #", "#   #", " ####", "    #", "   # ", " ##  ",
        ],
        '.' => [
            "     ", "     ", "     ", "     ", "     ", " ##  ", " ##  ",
        ],
        '-' => [
            "     ", "     ", "     ", "#####", "     ", "     ", "     ",
        ],
        _ => ["     "; 7],
    }
}

/// An RGB picture for the lineup.
#[cfg(test)]
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 3]>,
}

#[cfg(test)]
impl Canvas {
    fn new(w: usize, h: usize, fill: [f32; 3]) -> Self {
        Self {
            w,
            h,
            px: vec![fill; w * h],
        }
    }
    fn blend(&mut self, x: i64, y: i64, c: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        let d = &mut self.px[y as usize * self.w + x as usize];
        for i in 0..3 {
            d[i] = c[i] * a + d[i] * (1.0 - a);
        }
    }
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: [f32; 3]) {
        for j in y..y + h {
            for i in x..x + w {
                self.blend(i, j, c, 1.0);
            }
        }
    }
    /// Text in 5 x 7 letters `size` pixels to a dot, its top left at (x, y).
    fn text(&mut self, x: i64, y: i64, size: i64, s: &str, c: [f32; 3]) {
        for (n, ch) in s.to_uppercase().chars().enumerate() {
            for (j, row) in glyph(ch).iter().enumerate() {
                for (i, dot) in row.chars().enumerate() {
                    if dot == '#' {
                        self.rect(
                            x + (n as i64 * 6 + i as i64) * size,
                            y + j as i64 * size,
                            size,
                            size,
                            c,
                        );
                    }
                }
            }
        }
    }
}

/// One puppet at rest for the lineup: its parts (all but `hide`), and extra copies of parts at offsets in world
/// units with a height scale (the moray's segments, strung by code).
#[cfg(test)]
struct Puppet {
    label: &'static str,
    json: &'static str,
    atlas: &'static [u8],
    scale: f32,
    hide: &'static [&'static str],
    copies: Vec<(&'static str, f32, f32)>,
}

/// Draw a puppet with its lowest pixel on `ground` and its left edge at `left` (pixels),
/// `k` pixels a world unit; returns its drawn width in pixels.
#[cfg(test)]
fn draw(canvas: &mut Canvas, p: &Puppet, left: i64, ground: i64, k: f32) -> i64 {
    let file: PartsFile = serde_json::from_str(p.json).expect("a parts file is valid");
    let image = Image::from_buffer(
        p.atlas,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .expect("an atlas is a png");
    let aw = image.width() as usize;
    let data = image.data.as_ref().expect("decoded pixels");
    // Every piece to draw: (part, x offset in art units, height scale), back to front.
    let mut pieces: Vec<(&crate::puppet::PartDef, f32, f32)> = file
        .parts
        .iter()
        .filter(|q| !p.hide.contains(&q.name.as_str()))
        .map(|q| (q, 0.0, 1.0))
        .collect();
    for &(name, dx, sy) in &p.copies {
        let q = file
            .parts
            .iter()
            .find(|q| q.name == name)
            .expect("a copied part exists");
        pieces.insert(0, (q, dx / p.scale, sy));
    }
    pieces.sort_by(|a, b| a.0.z.total_cmp(&b.0.z));
    // Where the opaque pixels are: art units, for placing.
    let opaque = |q: &crate::puppet::PartDef, i: usize, j: usize| {
        let [rx, ry, ..] = q.rect.map(|v| v as usize);
        data[((ry + j) * aw + rx + i) * 4 + 3]
    };
    let (mut x0, mut x1, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for &(q, dx, sy) in &pieces {
        let [_, _, rw, rh] = q.rect.map(|v| v as usize);
        let mid = q.bbox[1] + q.bbox[3] / 2.0;
        for j in 0..rh {
            for i in 0..rw {
                if opaque(q, i, j) >= 128 {
                    let x = q.bbox[0] + dx + (i as f32 + 0.5) / rw as f32 * q.bbox[2];
                    let y = mid + (q.bbox[1] + (j as f32 + 0.5) / rh as f32 * q.bbox[3] - mid) * sy;
                    (x0, x1, y1) = (x0.min(x), x1.max(x), y1.max(y));
                }
            }
        }
    }
    let ks = k * p.scale;
    for &(q, dx, sy) in &pieces {
        let [rx, ry, rw, rh] = q.rect.map(|v| v as usize);
        let mid = q.bbox[1] + q.bbox[3] / 2.0;
        let top = mid + (q.bbox[1] - mid) * sy;
        let (ox, oy) = (
            ((q.bbox[0] + dx - x0) * ks) as i64,
            ((top - y1) * ks) as i64,
        );
        let (ow, oh) = (
            (q.bbox[2] * ks).ceil() as i64,
            (q.bbox[3] * sy * ks).ceil() as i64,
        );
        for j in 0..oh {
            for i in 0..ow {
                let ax = rx + (((i as f32 + 0.5) / ow as f32) * rw as f32) as usize;
                let ay = ry + (((j as f32 + 0.5) / oh as f32) * rh as f32) as usize;
                let s = &data[(ay * aw + ax) * 4..(ay * aw + ax) * 4 + 4];
                let lin = |v: u8| (f32::from(v) / 255.0).powf(2.2);
                canvas.blend(
                    left + ox + i,
                    ground + oy + j,
                    [lin(s[0]), lin(s[1]), lin(s[2])],
                    f32::from(s[3]) / 255.0,
                );
            }
        }
    }
    ((x1 - x0) * ks).ceil() as i64
}

/// Every animal beside the crab on one ruler, in the oil paint the game shows:
/// `cargo test --bin crabscape sizes::lineup -- --ignored` writes `docs/design/creatures/lineup.png`.
#[test]
#[ignore = "writes the README's lineup picture"]
fn lineup() {
    use crabsim::{Kind, Tuning};
    macro_rules! oil {
        ($n:literal) => {
            (
                include_str!(concat!("../assets/creatures/", $n, "-parts.json")),
                &include_bytes!(concat!("../assets/creatures/", $n, "-atlas-oil.png"))[..],
            )
        };
    }
    let t = Tuning::default();
    let body = |k: Kind| k.tuning(&t).body;
    let puppet = |label, (json, atlas): (&'static str, &'static [u8]), scale, hide| Puppet {
        label,
        json,
        atlas,
        scale,
        hide,
        copies: Vec::new(),
    };
    let crab = || {
        puppet(
            "our crab",
            (
                include_str!("../assets/crab/crab-parts.json"),
                &include_bytes!("../assets/crab/crab-atlas-oil.png")[..],
            ),
            crate::puppet::ART_SCALE,
            &[
                "back-carapace",
                "turn45-carapace",
                "turn90-carapace",
                "turn135-carapace",
                "pizza-arm",
                "pizza-fore",
                "cheliped-R-propodus-scissor",
                "cheliped-R-dactyl-scissor",
            ],
        )
    };
    let moray_scale = body(Kind::Moray).0 * crabsim::tuning::SEA_MORAY_SCALE / 44.0;
    let mut moray = puppet(
        "snowflake moray",
        oil!("snowflake-moray"),
        crate::critters::MORAY_ART_SCALE * moray_scale,
        &["segment-a", "segment-b", "segment-c"],
    );
    let head_x = {
        let f: PartsFile = serde_json::from_str(moray.json).unwrap();
        let h = f.parts.iter().find(|q| q.name == "head").unwrap();
        (h.bbox[0] + h.bbox[2] / 2.0) * moray.scale
    };
    for n in 0..crate::critters::NECK {
        let name = ["segment-a", "segment-b", "segment-c"][n as usize % 3];
        let f: PartsFile = serde_json::from_str(moray.json).unwrap();
        let s = f.parts.iter().find(|q| q.name == name).unwrap();
        let mid = (s.bbox[0] + s.bbox[2] / 2.0) * moray.scale;
        let at = head_x - crate::critters::moray_link_at(n) * moray_scale;
        moray
            .copies
            .push((name, at - mid, crate::critters::moray_taper(n)));
    }
    let rows: Vec<Vec<Puppet>> = vec![
        vec![
            crab(),
            puppet(
                "snapping shrimp",
                oil!("snapping-shrimp"),
                fitted(oil!("snapping-shrimp").0, body(Kind::Snapper).0),
                &[],
            ),
            puppet(
                "amphipod",
                oil!("amphipod-swarm"),
                crate::creatures::amphipods::ART_SCALE,
                &["cloud"],
            ),
            puppet(
                "hermit crab",
                oil!("hermit-crab"),
                crate::creatures::hermit::ART_SCALE,
                &[],
            ),
            puppet(
                "bobbit worm",
                oil!("bobbit-worm"),
                crate::creatures::bobbit::ART_SCALE,
                &[],
            ),
            puppet(
                "mantis shrimp",
                oil!("mantis-shrimp"),
                fitted(oil!("mantis-shrimp").0, body(Kind::Mantis).0),
                &[],
            ),
            puppet(
                "alarm jelly",
                oil!("alarm-jelly"),
                crate::alarm_jelly::ART_SCALE,
                &[],
            ),
            puppet(
                "viperfish",
                oil!("viperfish"),
                crate::creatures::viperfish::ART_SCALE,
                &[],
            ),
            puppet(
                "hagfish",
                oil!("hagfish"),
                crate::creatures::hagfish::ART_SCALE,
                &["pores0", "pores1", "pores2", "pores3", "pores4", "pores5"],
            ),
            puppet(
                "octopus, arms not shown",
                oil!("common-octopus"),
                crate::octopus::ART_SCALE,
                &[
                    "mantle-pale",
                    "head-pale",
                    "camo",
                    "camo-eye",
                    "web",
                    "arm-0",
                    "arm-1",
                    "arm-2",
                    "arm-3",
                    "arm-4",
                    "arm-5",
                    "arm-6",
                    "arm-7",
                ],
            ),
        ],
        vec![
            crab(),
            puppet(
                "triggerfish",
                oil!("queen-triggerfish"),
                crate::triggerfish::ART_SCALE,
                &[],
            ),
            puppet(
                "giant isopod",
                oil!("giant-isopod"),
                crate::creatures::isopod::ART_SCALE,
                &[],
            ),
            puppet(
                "lobster",
                oil!("american-lobster"),
                crate::creatures::lobster::ART_SCALE,
                &[],
            ),
            moray,
        ],
        vec![
            crab(),
            puppet(
                "loggerhead turtle",
                oil!("loggerhead-turtle"),
                crate::creatures::turtle::ART_SCALE,
                &[],
            ),
            puppet(
                "humboldt squid",
                oil!("humboldt-squid"),
                crate::creatures::squid::ART_SCALE,
                &["body-flash", "head-flash"],
            ),
        ],
        vec![
            crab(),
            puppet(
                "goliath grouper",
                oil!("goliath-grouper"),
                crate::creatures::grouper::ART_SCALE,
                &[],
            ),
        ],
    ];
    // 4 pixels a centimetre.
    let k = 4.0 / CM;
    let (w, row_h, gap) = (1800_i64, 380_i64, 60_i64);
    let sea = [0.006, 0.03, 0.04];
    let ink = [0.85, 0.82, 0.7];
    let mut canvas = Canvas::new(w as usize, (row_h * rows.len() as i64 + 70) as usize, sea);
    canvas.text(30, 20, 3, "every animal beside the crab. 1 cm is 4 px", ink);
    for (r, row) in rows.iter().enumerate() {
        let ground = 70 + row_h * (r as i64 + 1) - 70;
        // The ruler: a tick every 10 cm, a long one every 50 with its number.
        canvas.rect(30, ground + 4, w - 60, 2, ink);
        for cm in 0..=((w - 60) / 4 / 10) {
            let x = 30 + cm * 40;
            let long = cm % 5 == 0;
            canvas.rect(x, ground + 4, 2, if long { 16 } else { 8 }, ink);
            if long {
                canvas.text(x + 4, ground + 12, 2, &format!("{}", cm * 10), ink);
            }
        }
        let mut left = 30;
        for (n, p) in row.iter().enumerate() {
            let width = draw(&mut canvas, p, left, ground, k);
            canvas.text(left, ground + 34 + (n as i64 % 2) * 18, 2, p.label, ink);
            left += width.max(6 * 2 * p.label.len() as i64 / 2) + gap;
        }
    }
    let lin = |v: f32| (v.max(0.0).powf(1.0 / 2.2) * 255.0).round() as u8;
    let mut rgba = Vec::with_capacity(canvas.px.len() * 4);
    for p in &canvas.px {
        rgba.extend_from_slice(&[lin(p[0]), lin(p[1]), lin(p[2]), 255]);
    }
    let image = Image::new(
        bevy::render::render_resource::Extent3d {
            width: canvas.w as u32,
            height: canvas.h as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image
        .try_into_dynamic()
        .expect("an rgba picture")
        .to_rgb8()
        .save(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/docs/design/creatures/lineup.png"
        ))
        .expect("the lineup is written");
}
