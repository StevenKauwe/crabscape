//! The rig-ready cut-out parts: front (in-game) view, neutral pose → `art/crab/crab-parts.svg`.

use regex::Regex;

use crate::crab::{Pose, TURN_VIEWS, back_shell, build_front, turn_shell};
use crate::geom::*;
use crate::item::{Item, Kind, transform_items};
use crate::palette;
use crate::svg::{ItemsOpt, SvgOut, Text, fixed1};

/// The part a piece hangs from ("root" for the carapace).
pub fn parent_of(id: &str) -> String {
    let re = |s: &str| Regex::new(s).unwrap();
    // The extra arm of the pizza look, a raised slice-arm on the shell.
    if id == "pizza-arm" {
        return "carapace".into();
    }
    if id == "pizza-fore" {
        return "pizza-arm".into();
    }
    if id.starts_with("pizza-fore-") {
        return "pizza-fore".into();
    }
    if let Some(m) = re(r"^(leg-[RL]4)-paddle-rib").captures(id) {
        return format!("{}-dactyl-paddle", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-(merus)-spine").captures(id) {
        return format!("{}-merus", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-carpus-spine").captures(id) {
        return format!("{}-carpus", &m[1]);
    }
    if let Some(m) =
        re(r"^(cheliped-[RL])-(palm-ridge|palm-belly|molar|propodus-fixedfinger)").captures(id)
    {
        return format!("{}-propodus-palm", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-dactyl-tip").captures(id) {
        return format!("{}-dactyl", &m[1]);
    }
    // Scissor-claw teeth: each row is a part riding on its finger.
    if let Some(m) = re(r"^(cheliped-[RL])-(dactyl|propodus)-scissor-tooth").captures(id) {
        return format!("{}-{}-scissor", &m[1], &m[2]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-dactyl-scissor$").captures(id) {
        return format!("{}-dactyl", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-propodus-scissor$").captures(id) {
        return format!("{}-propodus-palm", &m[1]);
    }
    // Gear and feats, each a part riding on what it is fitted to: the clam-shell guard on the palm (its ribs drawn
    // into it), glass grips and gripping hooks on the leg tips, the broad paddle on the paddle (its ribs too).
    if let Some(m) = re(r"^(cheliped-[RL])-propodus-guard-rib").captures(id) {
        return format!("{}-propodus-guard", &m[1]);
    }
    if let Some(m) = re(r"^(cheliped-[RL])-propodus-guard$").captures(id) {
        return format!("{}-propodus-palm", &m[1]);
    }
    if let Some(m) = re(r"^(leg-[RL][123])-dactyl-(glass|hook)$").captures(id) {
        return format!("{}-dactyl", &m[1]);
    }
    if let Some(m) = re(r"^(leg-[RL]4)-paddle-broad-rib").captures(id) {
        return format!("{}-paddle-broad", &m[1]);
    }
    if let Some(m) = re(r"^(leg-[RL]4)-paddle-broad$").captures(id) {
        return format!("{}-dactyl-paddle", &m[1]);
    }
    if let Some(m) = re(r"^(eye-[RL])-(cornea|glint)").captures(id) {
        return format!("{}-stalk", &m[1]);
    }
    if re(r"^maxilliped3-[RL]-suture").is_match(id) {
        return id.replacen("-suture", "", 1);
    }
    if let Some(m) = re(r"^(leg-[RL]\d)-(merus|carpus|propodus|dactyl)").captures(id) {
        return match &m[2] {
            "merus" => "carapace".into(),
            "carpus" => format!("{}-merus", &m[1]),
            "propodus" => format!("{}-carpus", &m[1]),
            _ => format!("{}-propodus", &m[1]),
        };
    }
    if let Some(m) = re(r"^(cheliped-[RL])-(merus|carpus|propodus|dactyl)").captures(id) {
        return match &m[2] {
            "merus" => "carapace".into(),
            "carpus" => format!("{}-merus", &m[1]),
            "propodus" => format!("{}-carpus", &m[1]),
            _ => format!("{}-propodus-palm", &m[1]),
        };
    }
    // The back view's shell (`crab::back_shell`): its pieces ride on it.
    if id.starts_with("back-") {
        return if id == "back-carapace" {
            "carapace".into()
        } else {
            "back-carapace".into()
        };
    }
    // The shell turned part way round (`crab::turn_shell`): the same.
    if let Some(m) = re(r"^(turn\d+)-").captures(id) {
        return if id == format!("{}-carapace", &m[1]) {
            "carapace".into()
        } else {
            format!("{}-carapace", &m[1])
        };
    }
    if re(r"^eye-|^antenn|^maxilliped|^face|^underside|^gear-helmet").is_match(id) {
        return "carapace".into();
    }
    if id == "carapace" {
        "root".into()
    } else {
        "carapace".into()
    }
}

/// `crab-parts.svg` for a pose (the hero pose for the game), with the shell seen from behind and turned part way
/// round last (the game shows them as the crab turns).
pub fn parts_svg(pose: &Pose) -> String {
    write_with(
        crab_items(pose),
        COMMENT,
        TITLE,
        parent_of,
        &palette::normal(),
    )
}

/// The same crab made of pizza (`pizza-crab-parts.svg`): every piece where the red crab's is (the game swaps
/// pictures piece for piece), in pizza colours, with pepperoni on the shell.
pub fn pizza_parts_svg(pose: &Pose) -> String {
    let mut pose = *pose;
    pose.pizza = true;
    write_with(
        crab_items(&pose),
        COMMENT,
        "CrabScape pizza crab · rig parts (front view)",
        parent_of,
        &palette::pizza(),
    )
}

/// The crab's pieces: front view, then the back and turned shells, then the pizza look's extra arm.
fn crab_items(pose: &Pose) -> Vec<Item> {
    let mut items = build_front(pose);
    items.extend(back_shell(pose));
    for yaw in TURN_VIEWS {
        items.extend(turn_shell(pose, yaw));
    }
    items.extend(pizza_arm());
    items
}

/// The eleventh limb, shown with the pizza look: a slice-arm raised from the top of the shell, a crust-coloured
/// stalk with a pepperoni slice of pizza at its tip. Drawn in both skins so their pieces match.
fn pizza_arm() -> Vec<Item> {
    let shoulder = [-8.0, -46.0];
    let elbow = [-30.0, -86.0];
    let tip = [-8.0, -122.0];
    let mut upper = Item::new(
        Kind::Shape,
        "pizza-arm",
        Some("pizza-arm"),
        limb(&[shoulder, elbow], |t| 11.0 - 2.0 * t, LimbOpt::default()),
        "leg",
    );
    upper.pv = Some(shoulder);
    let mut fore = Item::new(
        Kind::Shape,
        "pizza-fore",
        Some("pizza-fore"),
        limb(&[elbow, tip], |t| 9.0 - 3.0 * t, LimbOpt::default()),
        "leg",
    );
    fore.pv = Some(elbow);
    // The slice: a wedge of cheese with a crust rim, and a pepperoni on it.
    let wedge = vec![
        [tip[0] - 30.0, tip[1] - 4.0],
        [tip[0] + 6.0, tip[1] + 8.0],
        [tip[0] + 30.0, tip[1] - 22.0],
        [tip[0] + 14.0, tip[1] - 54.0],
        [tip[0] - 14.0, tip[1] - 46.0],
    ];
    let crust: Vec<P> = through(
        &[
            wedge[3],
            [tip[0] + 22.0, tip[1] - 62.0],
            [tip[0] - 4.0, tip[1] - 68.0],
            [tip[0] - 20.0, tip[1] - 56.0],
            wedge[4],
        ],
        4,
        false,
    );
    vec![
        upper,
        fore,
        Item::new(
            Kind::Shape,
            "pizza-fore-slice",
            Some("pizza-fore-slice"),
            wedge,
            "pizzaCheese",
        ),
        Item::new(
            Kind::Shape,
            "pizza-fore-crust",
            Some("pizza-fore-crust"),
            {
                let mut c = crust;
                c.push([tip[0] + 14.0, tip[1] - 54.0]);
                c
            },
            "pizzaCrust",
        ),
        Item::new(
            Kind::Shape,
            "pizza-fore-pepperoni",
            Some("pizza-fore-pepperoni"),
            ell(tip[0] - 2.0, tip[1] - 26.0, 9.0, 8.0, 14, 0.0),
            "pepperoni",
        ),
    ]
}

const COMMENT: &str = "<!-- CrabScape player crab: rig-ready cut-out parts, front (in-game) view, neutral pose.
  One <g> per part. data-pivot = the joint this part rotates about (its proximal end), in this file's pixels.
  data-parent = the part it hangs from. Draw order = document order. R/L = the crab's own right/left
  (R is screen-left in this view). Legs: 1-3 walking legs, 4 = swimming paddle (the 5th pereiopod).
  back-*: the shell seen from behind (posterior border nearest), shown in the game instead of the carapace and face
  when the crab turns its back. turn45-*, turn90-*, turn135-*: the shell turned that far round (three-quarters from
  the front, the side, three-quarters from behind), shown part way through the turn.
  pizza-*: the eleventh limb, a slice-arm shown with the pizza look. -->";
const TITLE: &str = "CrabScape player crab · rig parts (front view)";

/// A rig-ready parts file: `items` (in their own space) drawn at twice their size, one `<g>` per item with its
/// parent (`parent_of`) and pivot, under a `comment` and a `title`.
pub fn write(
    items: Vec<Item>,
    comment: &str,
    title: &str,
    parent_of: fn(&str) -> String,
) -> String {
    write_with(items, comment, title, parent_of, &palette::normal())
}

/// [`write`] in the colours of `pal`.
pub fn write_with(
    items: Vec<Item>,
    comment: &str,
    title: &str,
    parent_of: fn(&str) -> String,
    pal: &palette::Palette,
) -> String {
    let b = bbox(
        items
            .iter()
            .filter(|i| i.k != Kind::Glow)
            .flat_map(|i| i.pts.iter()),
    );
    let k = 2.0;
    let (w, h) = ((b.w * k + 120.0).ceil(), (b.h * k + 160.0).ceil());
    let m = chain(&[tr(60.0 - b.x0 * k, 100.0 - b.y0 * k), sc(k)]);
    let its: Vec<_> = transform_items(&items, m)
        .into_iter()
        .map(|mut it| {
            it.pv = it.pv.map(|p| ap(m, p));
            it
        })
        .collect();
    let mut s = SvgOut::new(w, h, None);
    s.raw(comment);
    s.text(
        20.0,
        40.0,
        title,
        &Text {
            size: 22.0,
            family: "Nimbus Sans, sans-serif",
            ..Default::default()
        },
    );
    // group per part id (one shape per group, lines too); add pivot + parent attributes
    let opt = ItemsOpt {
        scale: k,
        id_prefix: "p-".into(),
        ..Default::default()
    };
    for it in &its {
        let pv = it
            .pv
            .map(|p| format!(r#" data-pivot="{},{}""#, fixed1(p[0]), fixed1(p[1])))
            .unwrap_or_default();
        s.raw(format!(
            r#"<g id="{}" data-parent="{}"{pv}>"#,
            it.id,
            parent_of(&it.id)
        ));
        let mut one = it.clone();
        one.g = None;
        s.items(&[one], pal, &opt);
        s.raw("</g>");
    }
    // pivots as tiny markers in a hidden layer
    s.raw(r#"<g id="pivots" style="display:none">"#);
    for it in &its {
        if let Some(p) = it.pv {
            s.raw(format!(
                r##"<circle cx="{}" cy="{}" r="3" fill="#00A0FF"><title>{}</title></circle>"##,
                fixed1(p[0]),
                fixed1(p[1]),
                it.id
            ));
        }
    }
    s.raw("</g>");
    s.to_string()
        .replace(r#"<g id="p-misc" data-part="misc">"#, "<g>")
}
