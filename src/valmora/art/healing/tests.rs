use super::*;
use serde_json::{Value, json};
fn function(name: &str) -> &'static str {
    let source = include_str!("../../../../web/dragon/index.html");
    let tail = &source[source.find(&format!("function {name}(")).unwrap()..];
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
    panic!("source function");
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
        (Value::String(a), Value::String(b))
            if a.starts_with("rgba(") && b.starts_with("rgba(") =>
        {
            let parse = |s: &str| {
                s[5..s.len() - 1]
                    .split(',')
                    .map(|v| v.parse::<f64>().unwrap())
                    .collect::<Vec<_>>()
            };
            compare(&json!(parse(a)), &json!(parse(b)), path);
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
fn cache_key_covers_source_projects_workshop_and_device_scale() {
    let mut c = HealingCache::default();
    let mut done = BTreeSet::new();
    let st = HealingCraft::default();
    let dims = [600., 360., 2.];
    assert!(!c.matches(dims, &done, st));
    c.key = Some((dims, vec![], st));
    assert!(c.matches(dims, &done, st));
    done.insert("oaks".into());
    assert!(!c.matches(dims, &done, st));
    done.clear();
    assert!(!c.matches([600., 360., 1.], &done, st));
    assert!(!c.matches(dims, &done, HealingCraft { bellows: 2., ..st }));
}
#[test]
#[ignore = "requires macOS JavaScriptCore; exact source Healing static and animated streams"]
fn healing_matches_original_source_except_prohibited_full_canvas_overlays() {
    let harness = include_str!("../../../../crates/dragon-game/tests/drawing.rs");
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
        "drawWick",
        "mulberry",
        "paintHealing",
        "drawHealing",
    ] {
        script.push_str(function(name));
        script.push('\n');
    }
    script.push_str(r#"
let size=[600,360],done=new Set(),st={},wild=[],healFlash=null,healKey='',healx;
const healArt={width:0,height:0,getContext:()=>healx},PROJECTS=Array(18),HEAL_ISLES=[[0.66,0.15,0.05],[0.78,0.22,0.06],[0.91,0.16,0.07]],HEAL_LANTERNS=[[0.09,0.79],[0.15,0.77],[0.21,0.78],[0.31,0.8]],HEAL_SPARKLES=Array.from({length:26},(_,i)=>[0.63+((i*37)%100)/100*0.35,0.78+((i*53)%100)/100*0.2,i*1.7]);
const mixHex=(a,b,t)=>{const A=rgb(a),B=rgb(b);return `rgb(${A.map((v,i)=>Math.round(v+(B[i]-v)*t)).join(',')})`;};
const $=()=>null,prepCanvas=()=>({c:ctx,w:size[0],h:size[1]}),teamBuilt=()=>done,teamCraft=()=>st,allWild=()=>wild,hornOf=d=>d.horn,pick=a=>a[Math.floor(a.length*0.4)];
Recorder.prototype.drawImage=()=>{};
const originalPaint=paintHealing;paintHealing=()=>{};
"#);
    let projects = [
        "lanterns",
        "bell",
        "barns",
        "oaks",
        "stream",
        "meadow",
        "bridge",
        "hall",
        "beacon",
        "lighthouse",
        "harbor",
        "pools",
        "garden",
        "spring",
        "path",
        "bridges",
        "flowers",
        "star",
    ];
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for mask in 0..=21 {
        let done: BTreeSet<String> = if mask < 18 {
            std::iter::once(projects[mask].to_owned()).collect()
        } else {
            projects
                .iter()
                .take([0, 6, 14, 18][mask - 18])
                .map(|v| (*v).into())
                .collect()
        };
        for size in [[600., 360.], [320., 240.]] {
            let st = HealingCraft {
                tea: if mask % 2 == 0 { 2. } else { 0. },
                tonic: if mask % 3 == 0 { 1. } else { 0. },
                treats_made: mask % 2 == 0,
                saddles: mask % 4 == 0,
                armor: if mask % 5 == 0 { 2. } else { 0. },
                bellows: if mask % 6 == 0 { 3. } else { 0. },
            };
            let time = mask as f64 * 0.17;
            let wild:Vec<_>=(0..if mask%2==0{8}else{0}).map(|i|json!({"name":format!("Wild{i}"),"color":"#3f5ad0","kind":"flame","horn":"#ffd0a0"})).collect();
            let refs: Vec<_> = wild.iter().collect();
            let flash = Some(HealFlash {
                spot: [0.2, 0.6],
                started: 0.,
            });
            let mut background = Canvas::default();
            static_paint(&mut background, size, &done, st, &mut || 0.4);
            let mut moving = Canvas::default();
            animated(&mut moving, size, &done, st, &refs, flash, time);
            expected.push(json!([background.commands, moving.commands]));
            cases.push(json!({"size":size,"done":done,"craft":{"tea":st.tea,"tonic":st.tonic,"treatsMade":if st.treats_made{1}else{0},"saddles":st.saddles,"armor":st.armor,"bellows":st.bellows},"wild":wild,"time":time}));
        }
    }
    script.push_str(&format!("const cases={};print(JSON.stringify(cases.map(v=>{{size=v.size;done=new Set(v.done);st=v.craft;wild=v.wild;time=v.time;healFlash={{spot:[0.2,0.6],t:0}};healx=new Recorder();originalPaint(size[0],size[1],done,st);ctx=new Recorder();drawHealing();return [healx.commands.slice(0,-2),ctx.commands];}})));",json!(cases)));
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
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
    compare(&json!(expected), &json!(actual), "healing");
}

#[test]
fn static_cache_reuses_geometry_and_random_state_until_a_source_key_changes() {
    use bevy_egui::egui;
    let ctx = egui::Context::default();
    let mut cache = HealingCache::default();
    let done = BTreeSet::from(["garden".to_owned()]);
    let render = |width: f32, time: f64, cache: &mut HealingCache| {
        let mut out = ctx.run_ui(Default::default(), |ui| {
            paint_healing_scene(
                ui.painter(),
                egui::Rect::from_min_size(egui::pos2(20., 30.), egui::vec2(width, 120.)),
                cache,
                HealingInput {
                    done: &done,
                    craft: HealingCraft::default(),
                    wild: &[],
                    flash: None,
                },
                time,
            );
        });
        out.textures_delta.clear();
    };
    render(240., 0., &mut cache);
    let state = cache.rng;
    let count = cache.background.len();
    assert!(count > 100);
    render(240., 1., &mut cache);
    assert_eq!(cache.rng, state);
    assert_eq!(cache.background.len(), count);
    render(241., 1., &mut cache);
    assert_ne!(cache.rng, state);
    assert_eq!(cache.key.as_ref().unwrap().0[0], 241.);
}
