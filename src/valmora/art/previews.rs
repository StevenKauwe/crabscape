//! Source drawRiderStanding, drawEggShape and drawBuildPreview geometry.
use dragon_game::drawing::{
    Canvas, DragonArt, RiderArt, draw_crown, draw_dragon, draw_gear, draw_star, ell, hair_back,
    hair_top, shade,
};
use std::f64::consts::{PI, TAU};

fn legs(c: &mut Canvas, u: f64) {
    c.begin();
    c.move_to(-u * 0.6, -u * 3.4);
    c.line_to(-u * 0.8, -u * 0.3);
    c.move_to(u * 0.6, -u * 3.4);
    c.line_to(u * 0.8, -u * 0.3);
    c.stroke();
}
pub(super) fn standing(
    c: &mut Canvas,
    position: [f64; 2],
    h: f64,
    rd: &RiderArt,
    rank: usize,
    pose: &str,
    t: f64,
) {
    let u = h * (0.8 + 0.035 * rank as f64) / 10.;
    let bob = if pose == "cheer" {
        (t * 6.).sin().abs() * u * 0.6
    } else {
        0.
    };
    c.save();
    c.translate(position[0], position[1] - bob);
    c.cap("round");
    let ink = "#1a1424";
    let lw = 1_f64.max(u * 0.14);
    c.stroke_style(ink);
    c.width(u * 1.05 + lw * 2.);
    legs(c, u);
    c.stroke_style("#3a2a1c");
    c.width(u * 1.05);
    legs(c, u);
    for bx in [-0.9, 0.95] {
        c.fill_style("#2a1d12");
        ell(c, u * bx, -u * 0.2, u * 0.7, u * 0.35);
        c.stroke_style(ink);
        c.width(lw);
        c.stroke();
        c.fill_style("rgba(255,255,255,0.22)");
        ell(c, u * (bx + 0.25), -u * 0.32, u * 0.22, u * 0.09);
    }
    let dad = rd.daddio;
    if rank >= 1 || dad {
        c.fill_style(if dad {
            "#d9a520".into()
        } else {
            shade(&rd.cloak, -0.45)
        });
        c.begin();
        c.move_to(-u * 1.3, -u * 6.6);
        c.quad(
            -u * 2.8 - (t * 3.).sin() * u * 0.4,
            -u * 3.6,
            -u * 2.2,
            -u * 2.4,
        );
        c.line_to(u * 0.4, -u * 3.);
        c.close();
        c.fill();
    }
    let tunic = c.linear(
        [-u * 1.5, -u * 7.2, u * 1.5, -u * 3.2],
        &[(0., shade(&rd.cloak, 0.25)), (1., shade(&rd.cloak, -0.2))],
    );
    c.fill_style(tunic);
    c.begin();
    c.move_to(-u * 1.5, -u * 3.2);
    c.line_to(-u * 1.2, -u * 6.8);
    c.quad(0., -u * 7.4, u * 1.2, -u * 6.8);
    c.line_to(u * 1.5, -u * 3.2);
    c.close();
    c.fill();
    c.stroke_style(ink);
    c.width(lw);
    c.join("round");
    c.stroke();
    c.fill_style(if dad {
        "#ffcf3f".into()
    } else if rank >= 4 {
        "#5fe0d0".into()
    } else {
        shade(&rd.cloak, 0.25)
    });
    c.fill_rect(-u * 1.45, -u * 3.9, u * 2.9, u * 0.45);
    c.fill_style("#4a3220");
    c.fill_rect(-u * 1.38, -u * 5., u * 2.76, u * 0.42);
    c.fill_style("#e8c56a");
    c.fill_rect(u * 0.15, -u * 5.08, u * 0.55, u * 0.58);
    c.fill_style("#4a3220");
    c.fill_rect(u * 0.3, -u * 4.93, u * 0.25, u * 0.28);
    if rank >= 2 {
        c.stroke_style("#6b4423");
        c.width(u * 0.3);
        c.begin();
        c.move_to(-u, -u * 6.6);
        c.line_to(u, -u * 4.4);
        c.stroke();
        c.fill_style("#7a5230");
        c.fill_rect(u * 0.6, -u * 4.7, u * 0.95, u * 0.85);
    }
    if rank >= 6 {
        draw_star(c, -u * 0.55, -u * 5.6, u * 0.42, "#ffd76e", 0.);
    } else if rank >= 3 {
        c.fill_style("#ffd76e");
        ell(c, -u * 0.55, -u * 5.6, u * 0.3, u * 0.3);
    }
    let shoulders = [[-u * 1.1, -u * 6.4], [u * 1.1, -u * 6.4]];
    let hands = match pose {
        "cheer" => [[-u * 2.2, -u * 9.6], [u * 2.2, -u * 9.6]],
        "point" => [[-u * 1.6, -u * 3.9], [u * 3.6, -u * 8.6]],
        "wave" => [
            [-u * 1.6, -u * 3.9],
            [u * 2.6 + (t * 8.).sin() * u * 0.6, -u * 9.2],
        ],
        _ => [[-u * 1.6, -u * 3.9], [u * 1.6, -u * 3.9]],
    };
    for (w, col) in [(u * 0.9 + lw * 2., ink), (u * 0.9, rd.cloak.as_str())] {
        c.stroke_style(col);
        c.width(w);
        for i in 0..2 {
            c.begin();
            c.move_to(shoulders[i][0], shoulders[i][1]);
            c.line_to(hands[i][0], hands[i][1]);
            c.stroke();
        }
    }
    for [hx, hy] in hands {
        c.fill_style(if rank >= 5 { "#b4541e" } else { &rd.skin });
        c.begin();
        c.arc(hx, hy, u * 0.45, 0., TAU);
        c.fill();
        c.stroke_style(ink);
        c.width(lw);
        c.stroke();
    }
    let [bx, by] = hands[0];
    if rd.path == "shadow" {
        draw_gear(c, "shadow", bx, by - u * 1.2, u, true);
    } else if rd.path == "forge" {
        c.stroke_style("#6b4423");
        c.width(u * 0.4);
        c.begin();
        c.move_to(bx, by);
        c.line_to(bx - u * 0.3, by + u * 2.4);
        c.stroke();
        c.fill_style("#9aa3b3");
        c.fill_rect(bx - u, by + u * 2.1, u * 1.5, u * 0.9);
    } else {
        draw_gear(c, &rd.path, bx, by, u * 1.1, true);
    }
    hair_back(c, 0., -u * 8.3, u * 1.25, rd);
    let face = c.radial(
        [u * 0.3, -u * 8.7, u * 0.2, 0., -u * 8.3, u * 1.4],
        &[(0., shade(&rd.skin, 0.12)), (1., shade(&rd.skin, -0.12))],
    );
    c.fill_style(face);
    ell(c, 0., -u * 8.3, u * 1.25, u * 1.3);
    c.stroke_style(ink);
    c.width(lw);
    c.stroke();
    c.fill_style(shade(&rd.skin, -0.1));
    ell(c, -u, -u * 8.2, u * 0.22, u * 0.32);
    hair_top(c, 0., -u * 8.3, u * 1.25, rd);
    c.fill_style("rgba(255,120,130,0.35)");
    ell(c, u * 0.15, -u * 7.75, u * 0.25, u * 0.15);
    ell(c, u, -u * 7.75, u * 0.18, u * 0.13);
    for ex in [0.3, 0.85] {
        c.fill_style("#fff");
        ell(c, u * ex, -u * 8.3, u * 0.21, u * 0.25);
        c.fill_style("#3a2a1c");
        ell(c, u * (ex + 0.04), -u * 8.27, u * 0.13, u * 0.17);
        c.fill_style("#fff");
        ell(c, u * (ex + 0.08), -u * 8.36, u * 0.05, u * 0.05);
    }
    c.stroke_style(shade(&rd.hair, -0.2));
    c.width(u * 0.12);
    c.begin();
    c.move_to(u * 0.12, -u * 8.72);
    c.line_to(u * 0.48, -u * 8.78);
    c.move_to(u * 0.7, -u * 8.78);
    c.line_to(u * 1.02, -u * 8.72);
    c.stroke();
    c.stroke_style(shade(&rd.skin, -0.3));
    c.width(u * 0.1);
    c.begin();
    c.move_to(u * 0.62, -u * 8.15);
    c.line_to(u * 0.7, -u * 7.98);
    c.line_to(u * 0.58, -u * 7.95);
    c.stroke();
    if dad {
        c.fill_style("#14141c");
        for ex in [0.3, 0.85] {
            ell(c, u * ex, -u * 8.3, u * 0.32, u * 0.24);
        }
        c.stroke_style("#14141c");
        c.width(u * 0.12);
        c.begin();
        c.move_to(u * 0.45, -u * 8.35);
        c.line_to(u * 0.7, -u * 8.35);
        c.move_to(0., -u * 8.4);
        c.line_to(-u * 0.9, -u * 8.6);
        c.stroke();
        c.fill_style("rgba(255,255,255,0.8)");
        ell(c, u * 0.22, -u * 8.4, u * 0.08, u * 0.05);
        ell(c, u * 0.77, -u * 8.4, u * 0.08, u * 0.05);
    }
    c.stroke_style("#9c5a3c");
    c.width(u * 0.15);
    c.begin();
    c.arc(u * 0.55, -u * 7.8, u * 0.4, 0.2 * PI, 0.8 * PI);
    c.stroke();
    if rd.crown {
        draw_crown(c, 0., -u * 9.3, u * 0.85);
    }
    c.restore();
}

fn egg(c: &mut Canvas, x: f64, y: f64, r: f64, t: f64) {
    c.save();
    c.translate(x, y);
    c.rotate((t * 3.).sin() * 0.08);
    let g = c.radial(
        [0., -r, r * 0.2, 0., -r, r * 2.4],
        &[
            (0., "rgba(255,215,110,0.45)".into()),
            (1., "rgba(255,215,110,0)".into()),
        ],
    );
    c.fill_style(g);
    ell(c, 0., -r, r * 2.4, r * 2.4);
    c.fill_style("#efe3c8");
    ell(c, 0., -r, r * 0.8, r);
    c.fill_style("#c9b8ff");
    ell(c, -r * 0.25, -r * 1.3, r * 0.16, r * 0.13);
    ell(c, r * 0.3, -r * 0.8, r * 0.2, r * 0.15);
    ell(c, -r * 0.1, -r * 0.45, r * 0.12, r * 0.1);
    c.restore();
}

fn builder_scene(
    c: &mut Canvas,
    size: [f64; 2],
    draft: &serde_json::Value,
    rank: usize,
    step: usize,
    time: f64,
) {
    let [w, h] = size;
    let get = |key: &str, default: &str| draft[key].as_str().unwrap_or(default).to_owned();
    let rd = RiderArt {
        skin: get("skin", "#f0c49c"),
        hair: get("hair", "#5a3418"),
        style: get("style", "short"),
        cloak: get("cloak", "#3a3f4f"),
        path: draft["path"]
            .as_str()
            .filter(|v| !v.is_empty())
            .unwrap_or("shadow")
            .into(),
        crown: draft["crown"] == true,
        daddio: false,
    };
    let gr = c.linear(
        [0., 0., 0., h],
        &[(0., "#0b1a44".into()), (1., "#22386e".into())],
    );
    c.fill_style(gr);
    c.fill_rect(0., 0., w, h);
    c.fill_style("rgba(255,246,221,0.7)");
    for k in 0..22 {
        c.fill_rect((k * 61) as f64 % w, (k * 37) as f64 % (h * 0.5), 2., 2.);
    }
    c.fill_style("#16244f");
    c.begin();
    c.move_to(0., h);
    c.line_to(0., h * 0.86);
    c.quad(w * 0.5, h * 0.76, w, h * 0.86);
    c.line_to(w, h);
    c.close();
    c.fill();
    if step == 0 {
        standing(c, [w * 0.42, h * 0.9], h * 0.62, &rd, rank, "wave", time);
        egg(c, w * 0.8, h * 0.86, h * 0.07, time);
        return;
    }
    let ds = (w * 0.32).min(h * 0.36);
    let color = get("dcolor", "#3f5ad0");
    let rgb =
        dragon_game::drawing::source_rgb(&color).map(|v| (v as f64 * 0.3 + 0.5).floor() as u8);
    let eye = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let breaths = [
        ["#f2f8ff", "#8fc8ff", "#3f7dff"],
        ["#fffbe0", "#ffe07a", "#ff9a2e"],
        ["#fff6c2", "#ffb347", "#ff3f1f"],
        ["#f4fff0", "#9dffb0", "#2fd67a"],
        ["#fbf2ff", "#d6a8ff", "#9b5cff"],
        ["#fff0f8", "#ffb0d8", "#ff5fae"],
        ["#ffffff", "#ffe9a0", "#ffc400"],
    ];
    let mut opts = DragonArt {
        horn: Some(get("horn", "#ffe7a6")),
        eye: Some(eye),
        fire_cols: Some(
            breaths[draft["breath"].as_u64().unwrap_or(0).min(6) as usize].map(str::to_owned),
        ),
        ..Default::default()
    };
    standing(c, [w * 0.16, h * 0.9], h * 0.4, &rd, rank, "point", time);
    draw_dragon(
        c,
        w * 0.38,
        h * 0.84 - ds * 0.12 + (time * 2.).sin() * 2.,
        ds * 0.32,
        &color,
        1.,
        time * 1.4,
        &opts,
    );
    opts.fire = time % 2.4 < 0.7;
    draw_dragon(
        c,
        w * 0.66,
        h * 0.42 + (time * 2.).sin() * 6.,
        ds,
        &color,
        1.,
        time,
        &opts,
    );
}

pub fn builder_preview(
    ui: &mut bevy_egui::egui::Ui,
    draft: &serde_json::Value,
    profile: &serde_json::Value,
    step: usize,
) {
    use bevy_egui::egui;
    let w = ui.available_width().clamp(1., 500.);
    let h = (w * 0.6).min(240.);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, h), egui::Sense::hover());
    let time = ui.input(|i| i.time);
    let rank = ["m4", "m8", "m12", "m16", "m20", "m24"]
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            profile["cleared"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v.as_str() == Some(id)))
        })
        .map(|(i, _)| i + 1)
        .max()
        .unwrap_or(0);
    let mut c = Canvas::default();
    c.translate(rect.min.x as f64, rect.min.y as f64);
    builder_scene(&mut c, [w as f64, h as f64], draft, rank, step, time);
    let painter = ui.painter().with_clip_rect(rect);
    super::paint(&painter, &c);
    if step == 1 {
        let ds = (w * 0.32).min(h * 0.36);
        for (label, x, y) in [
            ("Now", w * 0.38, h * 0.97),
            ("All grown up", w * 0.66, h * 0.42 + ds * 0.62),
        ] {
            painter.text(
                rect.min + egui::vec2(x, y),
                egui::Align2::CENTER_BOTTOM,
                label,
                egui::FontId::proportional((h * 0.04).round().max(11.)),
                egui::Color32::from_rgb(184, 194, 230),
            );
        }
    }
    ui.ctx().request_repaint();
}

/// Source roost drawMini: a fixed frame, rather than another simulation clock.
pub fn roost_preview(
    ui: &mut bevy_egui::egui::Ui,
    dragon: &serde_json::Value,
    profile: &serde_json::Value,
) {
    use bevy_egui::egui;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(96., 70.), egui::Sense::hover());
    let stage = super::stage(dragon["xp"].as_f64().unwrap_or(0.));
    let s = 20. + 6. * stage as f64;
    let build = &profile["build"];
    let get = |key: &str, default: &str| build[key].as_str().unwrap_or(default).to_owned();
    let bonded = dragon["bonded"] == true;
    let dad = bonded && get("name", "").trim().eq_ignore_ascii_case("daddio");
    let rider = (bonded
        && dragon_game::level_of(dragon["xp"].as_f64().unwrap_or(0.)) >= 8
        && profile["craft"]["saddles"] == true)
        .then(|| RiderArt {
            skin: get("skin", "#f0c49c"),
            hair: get("hair", "#5a3418"),
            style: get("style", "short"),
            cloak: get("cloak", "#3a3f4f"),
            path: get("path", "shadow"),
            crown: build["crown"] == true,
            daddio: dad,
        });
    let y = rect.height() as f64 / 2.
        + s * if rider.is_some() { 0.32 } else { 0.22 }
        + 1.2_f64.sin() * s * 0.05;
    let mut c = Canvas::default();
    c.translate(rect.min.x as f64, rect.min.y as f64);
    draw_dragon(
        &mut c,
        rect.width() as f64 / 2. + s * 0.15,
        y,
        s,
        dragon["color"].as_str().unwrap_or("#3f5ad0"),
        1.,
        0.6,
        &DragonArt {
            horn: Some(super::horn(dragon)),
            rider,
            dad,
            ..Default::default()
        },
    );
    super::paint(&ui.painter().with_clip_rect(rect), &c);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    fn source_function(name: &str) -> &'static str {
        let source = include_str!("../../../web/dragon/index.html");
        let tail = &source[source
            .find(&format!("function {name}("))
            .expect("source function")..];
        let open = tail.find(") {").expect("body") + 2;
        let mut depth = 0;
        for (i, ch) in tail.char_indices().skip_while(|(i, _)| *i < open) {
            if ch == '{' {
                depth += 1;
            }
            if ch == '}' {
                depth -= 1;
                if depth == 0 {
                    return &tail[..=i];
                }
            }
        }
        panic!("unterminated function");
    }
    fn compare(a: &Value, b: &Value) {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                let a = a.as_f64().unwrap();
                let b = b.as_f64().unwrap();
                assert!((a - b).abs() < 1e-8 * b.abs().max(1.), "{a} != {b}");
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for (a, b) in a.iter().zip(b) {
                    compare(a, b);
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.len(), b.len());
                for (k, v) in a {
                    compare(v, &b[k]);
                }
            }
            _ => assert_eq!(a, b),
        }
    }
    #[test]
    #[ignore = "requires macOS JavaScriptCore CLI; synthetic source preview oracle only"]
    fn builder_previews_match_original_canvas_commands() {
        // Reuse the recording Canvas used by core's geometry oracle; never a browser or save.
        let harness = include_str!("../../../crates/dragon-game/tests/drawing.rs");
        let start = harness.find("const TAU=Math.PI*2;").unwrap();
        let tail = &harness[start..];
        let end = tail.find("\"#,\n").unwrap();
        let mut script = tail[..end].to_owned();
        for name in [
            "rgb",
            "shade",
            "hexA",
            "ell",
            "tri",
            "circle",
            "drawStar",
            "wing",
            "hairBack",
            "hairTop",
            "drawGear",
            "drawCrown",
            "drawRider",
            "drawDragon",
            "drawRiderStanding",
            "drawEggShape",
            "drawBuildPreview",
            "shadeHex",
        ] {
            script.push_str(source_function(name));
            script.push('\n');
        }
        script.push_str("let draft,buildStep,rank;const $=()=>null;const prepCanvas=()=>({c:ctx,w:400,h:240});const draftRider=()=>({...draft,path:draft.path||'shadow',rank});const LOOK={breath:[['Blue',['#f2f8ff','#8fc8ff','#3f7dff']]]};\n");
        let mut cases = Vec::new();
        let mut expected = Vec::new();
        for style in ["short", "long", "braid", "curly", "hood"] {
            for path in ["shadow", "forge", "storm"] {
                for step in 0..2 {
                    let draft = json!({"skin":"#f0c49c","hair":"#5a3418","style":style,"cloak":"#3a3f4f","path":path,"crown":true,"dcolor":"#3f5ad0","horn":"#ffe7a6","breath":0});
                    for rank in [0, 3, 6] {
                        let mut c = Canvas::default();
                        builder_scene(&mut c, [400., 240.], &draft, rank, step, 0.4);
                        expected.push(json!(c.commands));
                        cases.push(json!({"draft":draft,"step":step,"rank":rank}));
                    }
                }
            }
        }
        script.push_str(&format!("const cases={};print(JSON.stringify(cases.map(v=>{{ctx=new Recorder();draft=v.draft;buildStep=v.step;rank=v.rank;time=0.4;drawBuildPreview();return ctx.commands;}})));",json!(cases)));
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let mut child = Command::new(
            "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
        )
        .args(["-e", "eval(JSON.parse(readline()))"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{}\n", json!(script)).as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let actual: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in expected.iter().zip(actual.iter()) {
            compare(a, b);
        }
    }
}
