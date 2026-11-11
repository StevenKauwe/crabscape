//! Source non-flight canvases, with their input resolution kept outside painting.
use bevy_egui::egui;
use dragon_game::drawing::{Canvas, DragonArt, RiderArt, circle, draw_dragon};
use serde_json::Value;
use std::f64::consts::{PI, TAU};

/// Only live story participants belong in crew; stored teammate profiles alone do not imply presence.
pub enum AdventureScene<'a> {
    Milestone,
    RiddleEgg {
        golden: bool,
    },
    Frozen {
        color: &'a str,
        melted: bool,
    },
    Story {
        id: &'a str,
        page: &'a Value,
        crew: &'a [&'a Value],
    },
    Hatch {
        baby: &'a Value,
    },
    Valley {
        wild: &'a [&'a Value],
    },
}
fn riddle_egg(c: &mut Canvas, [w, h]: [f64; 2], golden: bool, time: f64) {
    let rx = w * 0.36;
    let ry = h * 0.44;
    c.translate(w / 2., h * 0.52);
    c.rotate((time * 10.).sin() * 0.14 * (time * 2.2).sin().max(0.));
    c.begin();
    c.move_to(0., -ry);
    c.cubic(rx * 0.95, -ry, rx * 1.2, ry * 0.85, 0., ry * 0.85);
    c.cubic(-rx * 1.2, ry * 0.85, -rx * 0.95, -ry, 0., -ry);
    c.fill_style(if golden { "#ffd76e" } else { "#fff3d6" });
    c.fill();
    c.width(3.);
    c.stroke_style(if golden { "#fff8d8" } else { "#ffd76e" });
    c.stroke();
    c.fill_style(if golden { "#fff8d8" } else { "#62f0d4" });
    for [x, y, s] in [
        [-0.35, -0.2, 0.18],
        [0.3, 0.15, 0.22],
        [-0.1, 0.5, 0.14],
        [0.25, -0.5, 0.12],
    ] {
        circle(c, x * rx, y * ry, s * rx);
    }
}
fn frozen(c: &mut Canvas, [width, height]: [f64; 2], color: &str, melted: bool, time: f64) {
    let s = width * 0.28;
    let t = if melted { time } else { 0.4 };
    draw_dragon(
        c,
        width / 2. + s * 0.15,
        height / 2. + s * 0.22 + (t * 2.).sin() * s * 0.05,
        s,
        color,
        1.,
        t,
        &DragonArt::default(),
    );
    if melted || width == 0. {
        return;
    }
    let (x, y, w, h) = (width * 0.12, height * 0.06, width * 0.76, height * 0.88);
    c.fill_style("rgba(168,228,255,0.38)");
    c.stroke_style("rgba(225,246,255,0.95)");
    c.width(3.);
    c.begin();
    c.round_rect(x, y, w, h, 12.);
    c.fill();
    c.stroke();
    c.stroke_style("rgba(255,255,255,0.75)");
    c.width(2.5);
    c.cap("round");
    c.begin();
    c.move_to(x + w * 0.1, y + h * 0.3);
    c.line_to(x + w * 0.22, y + h * 0.12);
    c.move_to(x + w * 0.1, y + h * 0.5);
    c.line_to(x + w * 0.34, y + h * 0.14);
    c.move_to(x + w * 0.78, y + h * 0.9);
    c.line_to(x + w * 0.92, y + h * 0.7);
    c.stroke();
}

fn milestone(c: &mut Canvas, [w, h]: [f64; 2], time: f64) {
    for (i, color) in ["#3f5ad0", "#2fb6b2", "#ff6a3d", "#e8b923", "#6cc6f0"]
        .into_iter()
        .enumerate()
    {
        let a = time * 0.8 + i as f64 * 1.25;
        draw_dragon(
            c,
            w / 2. + a.cos() * w * 0.32,
            h / 2. + a.sin() * h * 0.25,
            18.,
            color,
            if a.sin() > 0. { -1. } else { 1. },
            time * 1.4 + i as f64,
            &DragonArt::default(),
        );
    }
    dragon_game::drawing::draw_star(
        c,
        w / 2.,
        h / 2.,
        16. + (time * 4.).sin() * 3.,
        "#ffd76e",
        time,
    );
}

fn rider(p: &Value) -> RiderArt {
    let b = &p["build"];
    let get = |k: &str, d: &str| b[k].as_str().unwrap_or(d).to_owned();
    RiderArt {
        hair: get("hair", "#5a3418"),
        skin: get("skin", "#f0c49c"),
        cloak: get("cloak", "#3a3f4f"),
        style: get("style", "short"),
        path: get("path", "shadow"),
        crown: b["crown"] == true,
        daddio: get("name", "").trim().eq_ignore_ascii_case("daddio"),
    }
}
fn rank(p: &Value) -> usize {
    ["m4", "m8", "m12", "m16", "m20", "m24"]
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            p["cleared"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(id)))
        })
        .map(|(i, _)| i + 1)
        .max()
        .unwrap_or(0)
}
pub(super) fn wick(c: &mut Canvas, x: f64, y: f64, s: f64, t: f64) {
    draw_dragon(
        c,
        x,
        y,
        s,
        "#8f8578",
        -1.,
        t * 0.4,
        &DragonArt {
            horn: Some("#e8dcc0".into()),
            ..Default::default()
        },
    );
    let fx = x - s * 0.5;
    let fy = y - s * 0.78;
    let k = 1. + (t * 9.).sin() * 0.14;
    let gl = c.radial(
        [fx, fy, 0., fx, fy, s * 0.5],
        &[
            (0., "rgba(255,220,130,0.8)".into()),
            (1., "rgba(255,220,130,0)".into()),
        ],
    );
    c.fill_style(gl);
    circle(c, fx, fy, s * 0.5);
    c.fill_style("#ffd76e");
    c.begin();
    c.move_to(fx, fy - s * 0.26 * k);
    c.quad(fx + s * 0.12, fy, fx, fy + s * 0.1);
    c.quad(fx - s * 0.12, fy, fx, fy - s * 0.26 * k);
    c.fill();
}
fn story(c: &mut Canvas, size: [f64; 2], realm: usize, alone: bool, crew: &[&Value], time: f64) {
    let [w, h] = size;
    let realm = realm.min(5);
    let sky = [
        ["#0b1a44", "#2c4a8a"],
        ["#06231c", "#2a5a46"],
        ["#10295a", "#7aa0d0"],
        ["#061a36", "#1d6a86"],
        ["#2a0a0a", "#a8461c"],
        ["#08062a", "#4a2c8a"],
    ][realm];
    let g = c.linear([0., 0., 0., h], &[(0., sky[0].into()), (1., sky[1].into())]);
    c.fill_style(g);
    c.fill_rect(0., 0., w, h);
    c.fill_style("#eaf2ff");
    circle(c, w * 0.86, h * 0.22, 16.);
    c.fill_style("rgba(255,246,221,0.8)");
    for i in 0..30 {
        c.fill_rect((i * 97) as f64 % w, (i * 53) as f64 % (h * 0.6), 2., 2.);
    }
    c.fill_style(
        [
            "#16244f", "#0f3326", "#dfeeff", "#0e3a4a", "#2a1410", "#e8e0ff",
        ][realm],
    );
    c.begin();
    c.move_to(0., h);
    c.line_to(0., h * 0.78);
    c.quad(w * 0.2, h * 0.68, w * 0.42, h * 0.82);
    c.quad(w * 0.75, h * 0.98, w, h * 0.84);
    c.line_to(w, h);
    c.close();
    c.fill();
    for (k, p) in crew.iter().take(4).enumerate() {
        super::previews::standing(
            c,
            [
                w * (0.08 + 0.08 * k as f64),
                h * (0.8 - 0.01 * (k % 2) as f64),
            ],
            h * 0.34,
            &rider(p),
            rank(p),
            if k % 2 != 0 { "point" } else { "cheer" },
            time + k as f64,
        );
    }
    for (k, p) in crew.iter().take(4).enumerate() {
        let st = super::stage(p["party"][0]["xp"].as_f64().unwrap_or(0.));
        let s = 14. + 22. * [0.62_f64, 0.8, 1., 1.18][st].powi(2);
        let k = k as f64;
        draw_dragon(
            c,
            w * (0.46 + k * 0.14) + (time * 0.8 + k).sin() * w * 0.03,
            h * (0.3 + (k % 2.) * 0.12) + (time * 1.3 + k * 2.).sin() * 8.,
            s,
            p["build"]["dcolor"].as_str().unwrap_or("#3f5ad0"),
            1.,
            time * 1.2 + k,
            &DragonArt {
                horn: Some(p["build"]["horn"].as_str().unwrap_or("#ffe7a6").into()),
                dad: rider(p).daddio,
                aura: st == 3,
                ..Default::default()
            },
        );
    }
    if !alone {
        wick(c, w * 0.9, h * 0.8, h * 0.13, time);
    }
}
fn hatch(c: &mut Canvas, size: [f64; 2], baby: &Value, time: f64) {
    let [w, h] = size;
    let s = w * 0.2;
    let t = time * 1.3;
    draw_dragon(
        c,
        w / 2. + s * 0.15,
        h / 2. + s * 0.22 + (t * 2.).sin() * s * 0.05,
        s,
        baby["color"].as_str().unwrap_or("#ff6a3d"),
        1.,
        t,
        &DragonArt {
            horn: Some(super::horn(baby)),
            aura: baby["kind"] == "star",
            ..Default::default()
        },
    );
}
fn valley(c: &mut Canvas, size: [f64; 2], wild: &[&Value], time: f64) {
    let [w, h] = size;
    let sky = c.linear(
        [0., 0., 0., h],
        &[
            (0., "#1d2f6a".into()),
            (0.7, "#e59a6a".into()),
            (1., "#f4c27a".into()),
        ],
    );
    c.fill_style(sky);
    c.fill_rect(0., 0., w, h);
    c.fill_style("#2f5a4a");
    c.begin();
    c.move_to(0., h);
    c.quad(w * 0.25, h * 0.55, w * 0.5, h * 0.75);
    c.quad(w * 0.75, h * 0.5, w, h * 0.7);
    c.line_to(w, h);
    c.close();
    c.fill();
    c.fill_style("#7fc2e8");
    c.begin();
    c.ellipse(w * 0.62, h * 0.9, w * 0.16, h * 0.06, 0., 0., TAU);
    c.fill();
    c.fill_style("#1f4034");
    c.begin();
    c.move_to(0., h);
    c.quad(w * 0.35, h * 0.72, w * 0.7, h * 0.92);
    c.line_to(w, h * 0.85);
    c.line_to(w, h);
    c.close();
    c.fill();
    for i in 0..(wild.len() / 2).min(8) {
        let nx = w * (0.1 + i as f64 * 0.11);
        let ny = h * (0.8 + (i % 2) as f64 * 0.06);
        c.fill_style("#7a5a3a");
        c.begin();
        c.ellipse(nx, ny, 14., 6., 0., 0., PI);
        c.fill();
        c.fill_style("#fff3d6");
        c.begin();
        c.ellipse(nx, ny - 2., 5., 6., 0., 0., TAU);
        c.fill();
    }
    for (i, d) in wild.iter().skip(wild.len().saturating_sub(24)).enumerate() {
        let (x, y, f) = wild_position(size, i, time);
        draw_dragon(
            c,
            x,
            y,
            16.,
            d["color"].as_str().unwrap_or("#ff6a3d"),
            f,
            time * 1.2 + i as f64,
            &DragonArt {
                horn: Some(super::horn(d)),
                ..Default::default()
            },
        );
    }
}
fn wild_position([w, h]: [f64; 2], i: usize, time: f64) -> (f64, f64, f64) {
    let sp = 0.6 + ((i * 37) % 10) as f64 / 20.;
    let ph = i as f64 * 1.7;
    let a = time * 0.25 * sp + ph;
    (
        w * (0.5 + 0.42 * a.sin()),
        h * (0.18 + 0.42 * (0.5 + 0.5 * (time * 0.4 * sp + ph * 1.3).sin())),
        if a.cos() >= 0. { 1. } else { -1. },
    )
}
/// Paint a source canvas inside an explicitly allocated UI rectangle. No state updates.
/// Returns Err for missing required presentation data instead of drawing invented characters.
pub fn paint_adventure_scene(
    painter: &egui::Painter,
    rect: egui::Rect,
    scene: AdventureScene<'_>,
    time: f64,
) -> Result<(), &'static str> {
    let mut c = Canvas::default();
    c.translate(rect.min.x as f64, rect.min.y as f64);
    let size = [rect.width() as f64, rect.height() as f64];
    match &scene {
        AdventureScene::Milestone => milestone(&mut c, size, time),
        AdventureScene::RiddleEgg { golden } => riddle_egg(&mut c, size, *golden, time),
        AdventureScene::Frozen { color, melted } => frozen(&mut c, size, color, *melted, time),
        AdventureScene::Story { id, page, crew } => {
            if crew.is_empty() || crew.iter().any(|p| !p["build"].is_object()) {
                return Err("Story art requires built profiles for the actual live crew");
            }
            let realm =
                mission_realm(id).unwrap_or_else(|| page["realm"].as_u64().unwrap_or(0) as usize);
            if realm > 5 {
                return Err("Unsupported story realm");
            }
            story(&mut c, size, realm, page["alone"] == true, crew, time);
        }
        AdventureScene::Hatch { baby } => {
            if baby["color"].as_str().is_none() || baby["kind"].as_str().is_none() {
                return Err("Hatch art requires the pending baby's color and kind");
            }
            hatch(&mut c, size, baby, time);
        }
        AdventureScene::Valley { wild } => valley(&mut c, size, wild, time),
    }
    let painter = painter.with_clip_rect(rect);
    super::paint(&painter, &c);
    if let AdventureScene::Valley { wild } = scene {
        if wild.is_empty() {
            painter.text(
                rect.min + egui::vec2(rect.width() / 2., rect.height() * 0.38),
                egui::Align2::CENTER_CENTER,
                "No dragons live here yet. Waiting for the first one...",
                egui::FontId::proportional(15.),
                egui::Color32::from_rgb(255, 243, 214),
            );
        }
        for (i, d) in wild.iter().skip(wild.len().saturating_sub(24)).enumerate() {
            let (x, y, _) = wild_position(size, i, time);
            painter.text(
                rect.min + egui::vec2(x as f32, (y + 9.) as f32),
                egui::Align2::CENTER_TOP,
                d["name"].as_str().unwrap_or(""),
                egui::FontId::proportional(11.),
                egui::Color32::from_rgb(255, 243, 214),
            );
        }
    }
    Ok(())
}

fn mission_realm(id: &str) -> Option<usize> {
    static REALMS: std::sync::OnceLock<Vec<(String, usize)>> = std::sync::OnceLock::new();
    let realms = REALMS.get_or_init(|| {
        dragon_game::missions::missions()
            .into_iter()
            .map(|m| (m.id, m.realm))
            .collect()
    });
    let base = id.strip_suffix('c').unwrap_or(id);
    realms
        .iter()
        .find(|(name, _)| name == base)
        .map(|(_, realm)| *realm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn function(name: &str) -> &'static str {
        let src = include_str!("../../../web/dragon/index.html");
        let tail = &src[src.find(&format!("function {name}(")).unwrap()..];
        let open = tail.find(") {").unwrap() + 2;
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
        panic!("function");
    }
    fn compare(a: &Value, b: &Value, path: &str) {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => {
                let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
                assert!((a - b).abs() < 1e-8 * b.abs().max(1.), "{path}: {a} != {b}");
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    compare(a, b, &format!("{path}[{i}]"));
                }
            }
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (k, v) in a {
                    compare(v, &b[k], &format!("{path}.{k}"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }
    #[test]
    #[ignore = "requires macOS JavaScriptCore CLI; synthetic source scene comparison"]
    fn adventure_scenes_match_live_original_source() {
        let harness = include_str!("../../../crates/dragon-game/tests/drawing.rs");
        let tail = &harness[harness.find("const TAU=Math.PI*2;").unwrap()..];
        let mut script = tail[..tail.find("\"#,\n").unwrap()].to_owned();
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
            "drawWick",
            "drawStoryArt",
            "drawMini",
            "drawValley",
            "drawEgg",
            "drawFrozen",
            "drawMilestoneArt",
        ] {
            script.push_str(function(name));
            script.push('\n');
        }
        script.push_str("let size=[400,190],storyId='',storyPage={},RIDERS=[],others=[],menuProfiles=[],ri=0,wild=[],frozen={};Date.now=()=>0;const FONT='sans-serif',$=()=>({getBoundingClientRect:()=>({width:size[0],height:size[1]}),getContext:()=>ctx}),prepCanvas=()=>({c:ctx,w:size[0],h:size[1]}),missionById=()=>null,storyCrew=()=>RIDERS.map((_,i)=>i),STAGES=[{scale:0.62},{scale:0.8},{scale:1},{scale:1.18}],stageOf=d=>d.stage,allWild=()=>wild,hornOf=d=>d.horn;REALMS=['hollow','wood','frost','sea','ember','stars'].map(id=>({id}));\n");
        let mut expected = Vec::new();
        let mut inputs = Vec::new();
        let profiles:Vec<Value>=(0..4).map(|i|json!({"build":{"name":if i==3{"Daddio"}else{"Nova"},"skin":"#f0c49c","hair":"#5a3418","style":(["short","long","curly","hood"][i]),"cloak":"#3a3f4f","path":"forge","crown":i>1,"dcolor":"#3f5ad0","horn":"#ffe7a6"},"party":[{"xp":([0.,100.,800.,3000.][i])}],"cleared":if i==0{vec![]}else{vec!["m12"]}})).collect();
        for realm in 0..6 {
            for alone in [false, true] {
                for count in [1, 4] {
                    let crew: Vec<_> = profiles[..count].iter().collect();
                    let mut c = Canvas::default();
                    story(&mut c, [400., 190.], realm, alone, &crew, 2.4);
                    expected.push(json!(c.commands));
                    let riders: Vec<_> = crew
                        .iter()
                        .map(|p| {
                            let rd = rider(p);
                            let mut v = serde_json::to_value(rd).unwrap();
                            v["rank"] = json!(rank(p));
                            v["color"] = p["build"]["dcolor"].clone();
                            v["horn"] = p["build"]["horn"].clone();
                            v
                        })
                        .collect();
                    let profs:Vec<_>=crew.iter().map(|p|json!({"party":[{"stage":super::super::stage(p["party"][0]["xp"].as_f64().unwrap())}]})).collect();
                    inputs.push(json!({"scene":"story","realm":realm,"alone":alone,"riders":riders,"profiles":profs}));
                }
            }
        }
        for kind in ["flame", "spark", "boulder", "frost", "star"] {
            let baby = json!({"kind":kind,"color":"#3f5ad0"});
            let mut c = Canvas::default();
            hatch(&mut c, [200., 130.], &baby, 2.4);
            expected.push(json!(c.commands));
            inputs.push(json!({"scene":"hatch","baby":baby,"horn":super::super::horn(&baby)}));
        }
        for count in [0, 1, 8, 30] {
            let wild:Vec<_>=(0..count).map(|i|json!({"name":format!("Dragon {i}"),"color":"#3f5ad0","kind":"flame","horn":"#ffd0a0"})).collect();
            let refs: Vec<_> = wild.iter().collect();
            let mut c = Canvas::default();
            valley(&mut c, [400., 200.], &refs, 2.4);
            expected.push(json!(c.commands));
            inputs.push(json!({"scene":"valley","wild":wild}));
        }
        for time in [0., 0.4, 1.25, 2.4] {
            let mut c = Canvas::default();
            milestone(&mut c, [200., 130.], time);
            expected.push(json!(c.commands));
            inputs.push(json!({"scene":"milestone","time":time}));
            for golden in [false, true] {
                let mut c = Canvas::default();
                riddle_egg(&mut c, [140., 160.], golden, time);
                expected.push(json!(c.commands));
                inputs.push(json!({"scene":"egg","time":time,"golden":golden}));
            }
            for melted in [false, true] {
                let mut c = Canvas::default();
                frozen(&mut c, [200., 130.], "#3f5ad0", melted, time);
                expected.push(json!(c.commands));
                inputs.push(json!({"scene":"frozen","time":time,"melted":melted}));
            }
        }
        script.push_str(&format!("const cases={};print(JSON.stringify(cases.map(v=>{{ctx=new Recorder();time=v.time??2.4;if(v.scene==='milestone'){{size=[200,130];drawMilestoneArt();}}else if(v.scene==='egg'){{size=[140,160];drawEgg(null,time,v.golden);}}else if(v.scene==='frozen'){{size=[200,130];RIDERS=[{{color:'#3f5ad0'}}];frozen={{who:0,until:v.melted?-1:1}};drawFrozen();}}else if(v.scene==='story'){{size=[400,190];storyPage={{realm:v.realm,alone:v.alone}};RIDERS=v.riders;me=v.profiles[0];others=v.profiles;drawStoryArt();}}else if(v.scene==='hatch'){{size=[200,130];drawMini(null,v.baby.color,40,time*1.3,{{horn:v.horn,aura:v.baby.kind==='star'}});}}else{{size=[400,200];wild=v.wild;drawValley();}}return ctx.commands;}})));",json!(inputs)));
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
        compare(&json!(expected), &json!(actual), "scenes");
    }
}
