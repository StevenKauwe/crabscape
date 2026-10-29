//! Materials, resolved per picture by palette.

use std::collections::BTreeMap;

use crate::geom::lerp;

/// A material: lit / base / shade colours, or one colour (ink, shadow, line).
#[derive(Clone, Debug, PartialEq)]
pub enum Mat {
    Tri([String; 3]),
    One(String),
}

pub type Palette = BTreeMap<&'static str, Mat>;

/// JS `Math.round`: halves go up.
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn hex(c: [f64; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0] as i64, c[1] as i64, c[2] as i64)
}
pub fn rgb(h: &str) -> [i64; 3] {
    let n = i64::from_str_radix(&h[1..], 16).unwrap_or(0);
    [n >> 16, (n >> 8) & 255, n & 255]
}
pub fn mix_hex(a: &str, b: &str, k: f64) -> String {
    let (pa, pb) = (rgb(a), rgb(b));
    hex(std::array::from_fn(|i| {
        js_round(pa[i] as f64 * (1.0 - k) + pb[i] as f64 * k)
    }))
}

fn tri(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.to_string(), b.to_string(), c.to_string()])
}

pub fn normal() -> Palette {
    let mut p = Palette::new();
    p.insert("shell", tri("#F07C4E", "#C9452C", "#7C2418"));
    p.insert("leg", tri("#F38A58", "#D05634", "#85301C"));
    p.insert("vent", tri("#F9D4AE", "#E7A37A", "#A6634A"));
    p.insert("stalk", tri("#F7C494", "#E39466", "#9E5A3C"));
    p.insert("cornea", tri("#4A3550", "#22172A", "#120A16"));
    p.insert("glint", tri("#FFF8EC", "#FFF3DE", "#EFE2C8"));
    p.insert("tip", tri("#5A3A30", "#2E1C17", "#1A0F0C"));
    p.insert("ink", Mat::One("#2A140F".into()));
    p.insert("shadow", Mat::One("#2C4950".into()));
    p.insert("cap", tri("#F2D27A", "#C8962E", "#6E4A16"));
    p.insert("capRed", tri("#E85A48", "#B42D24", "#5E1610"));
    p.insert("glassAmber", tri("#FBD58A", "#D9973A", "#8A5418"));
    p.insert("jarGlass", tri("#EAF2EC", "#C8D8D2", "#8BA19C"));
    p.insert("glow", tri("#FFF3B0", "#FFD45E", "#D89A22"));
    // The crab's gear: a clam's valve (cream, like the clams on the floor, lighter so it reads on the red claw) and
    // a glass sponge's spicules (icy, a little blue).
    p.insert("clamValve", tri("#F4EAD2", "#D2C3A2", "#8C7C5E"));
    p.insert("glassSponge", tri("#F4FCFF", "#C4E6F2", "#6E9FB4"));
    p.insert("line", Mat::One("#CFC7B2".into()));
    // v0.2: the face sits in the shade of the carapace brim (warm, not pale); pale cream undersides on the claws
    p.insert("face", tri("#D98A62", "#B9673F", "#7A3A24"));
    p.insert("belly", tri("#FCE2BC", "#F2BC8C", "#C27F55"));
    p.insert("corneaRim", tri("#8A6A9A", "#5B4470", "#2E2040"));
    // The snapping shrimp (its concept sheet's stylised olive; colour not verified).
    p.insert("shrimp", tri("#C9C27E", "#8E8A4E", "#5A5630"));
    p.insert("shrimpMottle", tri("#6E6A3C", "#5A5630", "#45421F"));
    p.insert("shrimpClaw", tri("#D9C98A", "#9C8448", "#5E4A26"));
    p.insert("shrimpLeg", tri("#EBD3A0", "#D4B07A", "#9C7646"));
    p.insert("shrimpTip", tri("#E5735A", "#C0452F", "#7A2414"));
    // The snowflake moray (its concept sheet's palette): cream skin, near-black blotches, yellow eye, nostrils and
    // spots, a dark red mouth.
    p.insert("moray", tri("#FFFBEF", "#EDE3C8", "#C9BC98"));
    p.insert("morayFlake", tri("#3A2E24", "#1E1712", "#120D0A"));
    p.insert("morayYellow", tri("#F8DC66", "#F2C531", "#B58C1A"));
    p.insert("morayMouth", tri("#9A3C3A", "#7A2A2A", "#4E1818"));
    p.insert("morayThroat", tri("#D27676", "#B85656", "#7A3030"));
    p.insert("morayTooth", tri("#FFFDF4", "#FFF6DE", "#E2D3AE"));
    // The peacock mantis shrimp (its concept sheet's palette).
    p.insert("mantis", tri("#86BE72", "#5B9452", "#2E5A34"));
    p.insert("mantisDeep", tri("#3E7444", "#2E5A34", "#1E3E24"));
    p.insert("mantisLeg", tri("#F8B07A", "#F08A4B", "#A8522A"));
    p.insert("mantisFan", tri("#F59A5E", "#E07A55", "#B8392C"));
    p.insert("mantisClub", tri("#F27C8A", "#D8475A", "#8E2235"));
    p.insert("mantisDactyl", tri("#FFD9CF", "#F7A3A0", "#C26A68"));
    p.insert("mantisShine", tri("#FFE4DE", "#FFD9CF", "#F7A3A0"));
    p.insert("mantisMeral", tri("#9A5BD0", "#6A2DA0", "#3C1660"));
    p.insert("mantisEye", tri("#5FA3AC", "#2D6670", "#163840"));
    p.insert("mantisBand", Mat::One("#E9F0C0".into()));
    p.insert("mantisStalk", tri("#C8E0A0", "#8DB86E", "#4E7A3E"));
    // The common octopus (its concept sheet's palette): reddish-brown skin, darker mottles and web, pale
    // suckers, a gold eye with a dark bar pupil; its camouflage in the rock's greys.
    p.insert("octopus", tri("#D27C56", "#AD5A3C", "#6E2E22"));
    p.insert("octopusDeep", tri("#94463A", "#6E3026", "#421A14"));
    p.insert("octopusWeb", tri("#A0503C", "#7C362A", "#4E1E16"));
    p.insert("octopusSucker", tri("#FCE8D8", "#F2CFB8", "#C99C84"));
    p.insert("octopusPale", tri("#FFF6EE", "#F7E6DA", "#DEC6B4"));
    p.insert("octopusEye", tri("#F6E79A", "#E8D274", "#B09A40"));
    p.insert("octopusPupil", tri("#2C2024", "#1A1214", "#0E0A0A"));
    p.insert("octopusCamo", tri("#ABA690", "#878170", "#5A5648"));
    p.insert("octopusCamoMottle", tri("#706C5A", "#5A5648", "#3E3B30"));
    p.insert("octopusCamoEye", tri("#D8CC86", "#BDAE62", "#857636"));
    p
}

fn map_pal(p: &Palette, f: impl Fn(&str) -> String, keys: &[&str]) -> Palette {
    let mut o = p.clone();
    for k in keys {
        if let Some(Mat::Tri(c)) = p.get(k) {
            let k: &'static str = p.keys().find(|x| *x == k).unwrap();
            o.insert(k, Mat::Tri([f(&c[0]), f(&c[1]), f(&c[2])]));
        }
    }
    o
}

fn lighten(h: &str, k: f64) -> String {
    let toward = [255.0, 248.0, 236.0];
    let c = rgb(h);
    hex(std::array::from_fn(|i| {
        js_round(lerp(c[i] as f64, toward[i], k))
    }))
}

/// Freshly moulted: soft, pale (artistic reading, see anatomy-notes).
pub fn soft() -> Palette {
    let mut p = map_pal(
        &normal(),
        |c| lighten(c, 0.5),
        &["shell", "leg", "vent", "stalk", "face", "belly"],
    );
    p.insert("ink", Mat::One("#8A5A48".into()));
    p
}
/// The same red crab seen at ~150 m: no red light left, so the red pigment reflects almost nothing (NOAA).
pub fn deep() -> Palette {
    let deepen = |c: &str| {
        let [r, g, b] = rgb(c).map(|v| v as f64);
        let y = 0.05 * r + 0.5 * g + 0.45 * b;
        hex([y * 0.25, y * 0.55, y * 0.7].map(|v| js_round((v * 0.6).min(255.0))))
    };
    let mut p = map_pal(
        &normal(),
        deepen,
        &[
            "shell",
            "leg",
            "vent",
            "stalk",
            "tip",
            "cornea",
            "face",
            "belly",
            "corneaRim",
        ],
    );
    p.insert("ink", Mat::One("#071216".into()));
    p
}
pub fn mid() -> Palette {
    let f = |c: &str| {
        let [r, g, b] = rgb(c).map(|v| v as f64);
        hex([r * 0.45, g * 0.75, b * 0.95 + 10.0].map(|v| js_round(v.min(255.0))))
    };
    map_pal(
        &normal(),
        f,
        &["shell", "leg", "vent", "stalk", "tip", "face", "belly"],
    )
}

pub fn by_name(name: &str) -> Palette {
    match name {
        "soft" => soft(),
        "deep" => deep(),
        "mid" => mid(),
        _ => normal(),
    }
}

pub fn one(p: &Palette, key: &str) -> String {
    match p.get(key) {
        Some(Mat::One(c)) => c.clone(),
        Some(Mat::Tri(c)) => c[1].clone(),
        None => String::new(),
    }
}
