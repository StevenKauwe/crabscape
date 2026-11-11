//! Synthetic damage fixtures only. Oracle stubs presentation, dropped objective
//! pieces and retirement world changes; retirement's player-state callback is real.
use dragon_game::damage::{DamageEvent, Hazard, Strike, Used, bonk, bump, is_safe, update_hazards};
use dragon_game::flight::{Player, Viewport};
use serde_json::{Value, json};

fn player() -> Player {
    Player {
        x: 100.0,
        y: 100.0,
        ..Player::default()
    }
}
fn view() -> Viewport {
    Viewport {
        width: 800.0,
        height: 700.0,
        scale: 40.0,
        ground_y: 600.0,
        top_limit: 40.0,
        dragon_scale: 1.0,
    }
}
fn hazard(uid: u64) -> Hazard {
    Hazard {
        uid,
        x: 100.0,
        y: 100.0,
        r: 5.0,
        life: 1.0,
        kind: "dust".into(),
        ..Hazard::default()
    }
}
fn strike(uid: u64) -> Strike {
    Strike {
        uid,
        x: 100.0,
        bolt: 1.0,
        ..Strike::default()
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn bump_and_armor_bonk_handle_exact_overlap_and_direction() {
    for armor in 0..=3 {
        let mut p = player();
        p.spin = 3.0;
        assert!(!bonk(&mut p, [97.0, 96.0], 40.0, f64::from(armor)));
        let factor = 1.0 - 0.2 * f64::from(armor);
        close(p.vx, 144.0 * factor);
        close(p.vy, 192.0 * factor);
        close(p.dizzy, 1.1 * factor);
        assert_eq!(p.hearts, 4);
        close(p.safe, 2.6);
        close(p.spin, 0.0);
        p.hearts = 0;
        assert!(bonk(&mut p, [100.0, 100.0], 40.0, f64::from(armor)));
        assert_eq!(p.hearts, 0);
        close(p.vx, 0.0);
        close(p.vy, 0.0);
    }
    let mut p = player();
    bump(&mut p, [97.0, 96.0], 40.0);
    close(p.vx, 120.0);
    close(p.vy, 160.0);
    assert_eq!(p.hearts, 5);
    bump(&mut p, [100.0, 100.0], 40.0);
    close(p.vx, 0.0);
    close(p.vy, 0.0);
}

#[test]
fn warning_and_expiry_orders_differ_for_hazards_and_strikes() {
    let mut p = player();
    let mut hazards = vec![Hazard {
        warn: 0.01,
        life: 0.01,
        vx: 100.0,
        ..hazard(1)
    }];
    let mut strikes = vec![Strike {
        warn: 0.01,
        bolt: 0.01,
        ..strike(2)
    }];
    let mut used = Used::default();
    let events = update_hazards(
        &mut hazards,
        &mut strikes,
        &mut p,
        0.02,
        view(),
        0.0,
        &mut used,
        |_| {},
    );
    assert_eq!(events, vec![DamageEvent::Zap]);
    assert_eq!(p.hearts, 5);
    close(hazards[0].x, 100.0);
    close(hazards[0].life, -0.01);
    close(strikes[0].bolt, 0.01);
    let events = update_hazards(
        &mut hazards,
        &mut strikes,
        &mut p,
        0.02,
        view(),
        0.0,
        &mut used,
        |_| {},
    );
    assert_eq!(events.len(), 1);
    assert_eq!(p.hearts, 4);
    assert!(hazards.is_empty() && strikes.is_empty());
    assert!(used.hazards.is_empty());
    assert!(used.strikes.contains(&2));
}

#[test]
fn safety_leaves_hazards_but_consumes_strikes_and_shield_consumes_both() {
    for shield in [false, true] {
        let mut p = Player {
            safe: 1.0,
            shield: if shield { 1.0 } else { 0.0 },
            ..player()
        };
        let mut hazards = vec![hazard(7)];
        let mut strikes = vec![strike(8)];
        let mut used = Used::default();
        let events = update_hazards(
            &mut hazards,
            &mut strikes,
            &mut p,
            0.01,
            view(),
            0.0,
            &mut used,
            |_| panic!("protected player"),
        );
        assert_eq!(p.hearts, 5);
        assert_eq!(hazards.is_empty(), shield);
        assert_eq!(used.hazards.contains(&7), shield);
        assert!(strikes[0].hit && used.strikes.contains(&8));
        assert_eq!(events.len(), usize::from(shield));
    }
    for field in ["dizzy", "safe", "shield", "sila", "shadow", "down", "out"] {
        let mut p = player();
        match field {
            "dizzy" => p.dizzy = 1.0,
            "safe" => p.safe = 1.0,
            "shield" => p.shield = 1.0,
            "sila" => p.sila = 1.0,
            "shadow" => p.shadow = 1.0,
            "down" => p.down = 1.0,
            _ => p.out = true,
        }
        assert!(is_safe(&p), "{field}");
    }
}

#[test]
fn reverse_collision_order_and_retirement_callback_are_observable() {
    let mut p = Player {
        hearts: 1,
        ..player()
    };
    let mut hazards = vec![hazard(1), hazard(2)];
    let mut strikes = vec![strike(3)];
    let mut used = Used::default();
    let mut calls = 0;
    let events = update_hazards(
        &mut hazards,
        &mut strikes,
        &mut p,
        0.01,
        view(),
        0.0,
        &mut used,
        |p| {
            calls += 1;
            assert_eq!(p.hearts, 0);
            p.out = true;
            p.dizzy = 0.0;
        },
    );
    assert_eq!(calls, 1);
    assert!(p.out);
    close(p.dizzy, 0.0);
    assert_eq!(events.len(), 1);
    assert_eq!(hazards.len(), 1);
    assert_eq!(hazards[0].uid, 1);
    assert_eq!(used.hazards.into_iter().collect::<Vec<_>>(), vec![2]);
    assert!(used.strikes.contains(&3));
}

fn project(p: &Player) -> Value {
    json!({"x":p.x,"y":p.y,"vx":p.vx,"vy":p.vy,"hearts":p.hearts,"dizzy":p.dizzy,"safe":p.safe,"shield":p.shield,"sila":p.sila,"shadow":p.shadow,"down":p.down,"out":p.out,"spin":p.spin})
}
fn event(e: DamageEvent) -> Value {
    match e {
        DamageEvent::Bonk { source, kind } => json!({"type":"bonk","source":source,"kind":kind}),
        DamageEvent::Shield { source } => json!({"type":"shield","source":source}),
        DamageEvent::Zap => json!({"type":"zap"}),
    }
}
fn source_function(name: &str) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(&format!("function {name}(")).unwrap()..];
    &tail[..tail.find("\nfunction ").unwrap()]
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() <= 1e-10 * b.abs().max(1.0),
                "{path}: Rust {a}, JS {b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, v) in a {
                compare(v, &b[k], &format!("{path}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_legacy_damage_source_matches_synthetic_cases() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for protection in 0..8 {
        for scenario in 0..16 {
            for armor in [0.0, 3.0] {
                let mut p = player();
                match protection {
                    1 => p.dizzy = 1.0,
                    2 => p.safe = 1.0,
                    3 => p.shield = 1.0,
                    4 => p.sila = 1.0,
                    5 => p.shadow = 1.0,
                    6 => p.down = 1.0,
                    7 => p.out = true,
                    _ => {}
                }
                let mut h = hazard(11);
                let mut s = strike(12);
                let dt = 0.02;
                match scenario {
                    1 => {
                        h.warn = 0.01;
                        s.warn = 0.01;
                    }
                    2 => {
                        h.life = 0.01;
                        s.bolt = 0.01;
                    }
                    3 => {
                        h.uid = 0;
                        s.uid = 0;
                    }
                    4 => {
                        h.x += 19.0;
                        s.x += 24.0;
                    }
                    5 => {
                        h.vx = 40.0;
                        h.vy = -30.0;
                        h.gravity = 80.0;
                    }
                    6 => {
                        s.y0 = Some(112.0);
                    }
                    7 => {
                        s.y0 = Some(111.99);
                    }
                    8 => {
                        p.hearts = 1;
                    }
                    9 => {
                        h.x = -61.0;
                        s.hit = true;
                    }
                    10 => h.y = 609.0,
                    11 => h.y = -81.0,
                    12 => {
                        h.warn = 0.01;
                        h.life = 0.01;
                        s.warn = 0.01;
                        s.bolt = 0.01;
                    }
                    13 => {
                        h.x = 860.0;
                        s.x = 124.0;
                    }
                    14 => {
                        h.x = 861.0;
                        s.x = 124.0;
                    }
                    15 => {
                        p.hearts = 0;
                    }
                    _ => {}
                }
                let mut hazards = vec![hazard(10), h];
                let mut strikes = vec![s];
                let initial = json!({"mode":"hazards","p":project(&p),"hazards":hazards,"strikes":strikes,"armor":armor,"dt":dt});
                let mut used = Used::default();
                let mut retired = 0;
                let events = update_hazards(
                    &mut hazards,
                    &mut strikes,
                    &mut p,
                    dt,
                    view(),
                    armor,
                    &mut used,
                    |p| {
                        if p.hearts == 0 {
                            retired += 1;
                            p.out = true;
                            p.dizzy = 0.0;
                        }
                    },
                );
                cases.push(initial);
                expected.push(json!({"p":project(&p),"hazards":hazards,"strikes":strikes,"usedHaz":used.hazards,"usedStrike":used.strikes,"retired":retired,"events":events.into_iter().map(event).collect::<Vec<_>>()}));
            }
        }
    }
    for mode in ["bump", "bonk"] {
        for armor in [0.0, 1.0, 2.0, 3.0] {
            for source in [[100.0, 100.0], [97.0, 96.0], [110.0, 120.0]] {
                for hearts in [0, 1, 5] {
                    let mut p = Player {
                        hearts,
                        spin: 2.0,
                        ..player()
                    };
                    let initial =
                        json!({"mode":mode,"p":project(&p),"source":source,"armor":armor});
                    let mut retired = 0;
                    let mut events = Vec::new();
                    if mode == "bump" {
                        bump(&mut p, source, 40.0);
                    } else {
                        let empty = bonk(&mut p, source, 40.0, armor);
                        events.push(json!({"type":"bonk","source":source,"kind":"fixture"}));
                        if empty {
                            retired += 1;
                            p.out = true;
                            p.dizzy = 0.0;
                        }
                    }
                    cases.push(initial);
                    expected.push(json!({"p":project(&p),"retired":retired,"events":events}));
                }
            }
        }
    }
    let source = include_str!("../../../web/dragon/index.html");
    let safe = source
        .lines()
        .find(|line| line.starts_with("const isSafe ="))
        .unwrap();
    let script = format!(
        r#"
const cases={cases};
let P,current,hazards,strikes,usedHaz,usedStrike,events,retired;
const S=40,W=800,groundY=600,ri=0,realmIdx=0,REALMS=[{{id:'sea'}}];
const R={{dragon:'Fixture'}},texts=[],reduceMotion=true,mStats=null;
let hitStop=0,shake=0;const mission={{guest:true,done:false,failT:0}};
const armorLv=()=>current.armor,scaleOf=()=>1,bonded=()=>({{}});
const dist=(x,y,a,b)=>Math.hypot(x-a,y-b),plog=()=>{{}},dropPiece=()=>{{}},say=()=>{{}},updateHud=()=>{{}},poofAt=()=>{{}};
const sfx={{thud:()=>{{}},bonk:()=>{{}},zap:()=>events.push({{type:'zap'}})}};
const burst=(x,y)=>events.push({{type:'shield',source:[x,y]}});
const retire=()=>{{retired++;P.out=true;P.dizzy=0;}};
{safe}
{bump}
{bonk}
const originalBonk=bonk;bonk=(x,y,kind)=>{{events.push({{type:'bonk',source:[x,y],kind}});originalBonk(x,y,kind);}};
{update}
const results=cases.map(c=>{{current=c;P=c.p;events=[];retired=0;
 if(c.mode==='hazards'){{hazards=c.hazards.map(h=>{{h.g=h.gravity;delete h.gravity;return h;}});strikes=c.strikes.map(s=>{{if(s.y0===null)delete s.y0;return s;}});usedHaz=new Set();usedStrike=new Set();updateHazards(c.dt);}}
 else if(c.mode==='bump')bump(...c.source);else bonk(...c.source,'fixture');
 delete P.sq;
 const result={{p:P,events,retired}};
 if(c.mode==='hazards')Object.assign(result,{{hazards:hazards.map(h=>{{h.gravity=h.g;delete h.g;return h;}}),strikes:strikes.map(s=>({{...s,y0:s.y0??null}})),usedHaz:Array.from(usedHaz).sort((a,b)=>a-b),usedStrike:Array.from(usedStrike).sort((a,b)=>a-b)}});
 return result;
}});print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        safe = safe,
        bump = source_function("bump"),
        bonk = source_function("bonk"),
        update = source_function("updateHazards")
    );
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
            .args(["-e", "eval(JSON.parse(readline()))"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("JavaScriptCore CLI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "oracle failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
    assert_eq!(actual.len(), expected.len());
    for (i, (rust, js)) in expected.iter().zip(actual).enumerate() {
        compare(rust, &js, &format!("case {i}: {}", cases[i]));
    }
}
