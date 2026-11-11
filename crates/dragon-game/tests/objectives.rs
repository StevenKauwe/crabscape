use dragon_game::{flight::Viewport, objectives::*};
use serde_json::{Value, json};
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
#[test]
fn objective_collision_order_and_cooldowns() {
    let v = view();
    let mut ob = Objective::new(&specification("m3", 3).unwrap(), v);
    let mut enemies = vec![Enemy {
        id: 1,
        kind: "cloud".into(),
        x: ob.x,
        y: ob.y,
        r: 20.,
        dir: 1.,
        ..Enemy::default()
    }];
    let mut hazards = vec![Hazard {
        id: 2,
        kind: "spark".into(),
        x: ob.x,
        y: ob.y,
        r: 10.,
        ..Hazard::default()
    }];
    let mut strikes = vec![Strike {
        id: 3,
        x: ob.x,
        ..Strike::default()
    }];
    let mut events = vec![];
    ob.update(
        0.,
        &mut World {
            view: v,
            mission_done: false,
            mission_failing: false,
            riders: &[],
            enemies: &mut enemies,
            hazards: &mut hazards,
            strikes: &mut strikes,
        },
        &mut || 0.5,
        &mut events,
    );
    assert_eq!(ob.hp, 6);
    assert_eq!(ob.hurt_t, 3.5);
    assert_eq!(enemies[0].ob_cd, 3.);
    assert_eq!(enemies[0].x, ob.x - v.scale * 0.5);
    assert!(hazards.is_empty());
    assert!(strikes[0].ob_hit);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Hurt { .. }))
            .count(),
        1
    );
}
#[test]
fn riders_pick_up_before_monsters_and_missing_carrier_drops() {
    let v = view();
    let mut ob = Objective::new(&specification("m17", 3).unwrap(), v);
    let rider = Rider {
        id: 2,
        x: ob.x,
        y: ob.y,
    };
    let mut enemies = vec![Enemy {
        id: 1,
        x: ob.x,
        y: ob.y,
        r: 20.,
        dir: 1.,
        ..Enemy::default()
    }];
    let mut hazards = vec![];
    let mut strikes = vec![];
    let mut events = vec![];
    ob.update(
        0.,
        &mut World {
            view: v,
            mission_done: false,
            mission_failing: false,
            riders: &[rider],
            enemies: &mut enemies,
            hazards: &mut hazards,
            strikes: &mut strikes,
        },
        &mut || 0.5,
        &mut events,
    );
    assert_eq!(ob.carrier, 2);
    assert_eq!(events, vec![Event::PickedUp(2)]);
    events.clear();
    ob.update(
        0.1,
        &mut World {
            view: v,
            mission_done: false,
            mission_failing: false,
            riders: &[],
            enemies: &mut enemies,
            hazards: &mut hazards,
            strikes: &mut strikes,
        },
        &mut || 0.25,
        &mut events,
    );
    assert_eq!(ob.carrier, -1);
    assert_eq!(ob.vx, -1.5 * v.scale);
    assert_eq!(ob.vy, -4. * v.scale);
    assert_eq!(ob.steal_cd, 1.);
    assert_eq!(events, vec![Event::Dropped]);
}
#[test]
fn escort_arrives_before_damage_and_carry_theft_clamps() {
    let v = view();
    let mut ob = Objective::new(&specification("m6", 3).unwrap(), v);
    let mut enemies = vec![Enemy {
        x: ob.to,
        y: ob.y,
        r: 100.,
        ..Enemy::default()
    }];
    let mut hazards = vec![];
    let mut strikes = vec![];
    let mut events = vec![];
    ob.update(
        45.,
        &mut World {
            view: v,
            mission_done: false,
            mission_failing: false,
            riders: &[],
            enemies: &mut enemies,
            hazards: &mut hazards,
            strikes: &mut strikes,
        },
        &mut || 0.5,
        &mut events,
    );
    assert!(ob.done);
    assert_eq!(ob.hp, 5);
    assert!(events.contains(&Event::Arrived));
    let mut ob = Objective::new(&specification("m18", 3).unwrap(), v);
    enemies[0] = Enemy {
        id: 5,
        x: ob.x,
        y: ob.y,
        r: 20.,
        dir: -1.,
        ..Enemy::default()
    };
    events.clear();
    ob.update(
        0.,
        &mut World {
            view: v,
            mission_done: false,
            mission_failing: false,
            riders: &[],
            enemies: &mut enemies,
            hazards: &mut hazards,
            strikes: &mut strikes,
        },
        &mut || 0.,
        &mut events,
    );
    assert_eq!(ob.x, v.scale * 1.5);
    assert_eq!(ob.steal_cd, 2.5);
    assert_eq!(events, vec![Event::Stolen(5)]);
}
fn legacy(name: &str) -> &str {
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find(&format!("function {name}(")).unwrap();
    let tail = &source[start..];
    &tail[..tail.find("\nfunction ").unwrap()]
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_all_specs_and_objective_updates_match() {
    let v = view();
    let mut cases = vec![];
    let mut expected = vec![];
    for (mid, stop) in [
        ("m3", 3),
        ("m6", 3),
        ("m11", 3),
        ("m14", 3),
        ("m15", 2),
        ("m17", 3),
        ("m18", 3),
        ("m21", 3),
    ] {
        let spec = specification(mid, stop).unwrap();
        let mut ob = Objective::new(&spec, v);
        let initial = serde_json::to_value(&ob).unwrap();
        let mut enemies = vec![Enemy {
            id: 1,
            kind: "cloud".into(),
            x: ob.x,
            y: ob.y,
            r: 20.,
            dir: 1.,
            ..Enemy::default()
        }];
        let mut hazards = vec![Hazard {
            id: 2,
            kind: "spark".into(),
            x: ob.x,
            y: ob.y,
            r: 10.,
            ..Hazard::default()
        }];
        let mut strikes = vec![Strike {
            id: 3,
            x: ob.x,
            ..Strike::default()
        }];
        let mut failing = false;
        let mut frames = vec![];
        let mut states = vec![];
        let enemy_initial = json!([{"uid":1,"type":"cloud","x":ob.x,"y":ob.y,"r":20,"dir":1,"dead":false,"calm":false,"boss":false,"stun":0,"obCd":0,"def":{"ground":false}}]);
        let hazard_initial = json!([{"uid":2,"kind":"spark","x":ob.x,"y":ob.y,"r":10,"warn":0}]);
        let strike_initial = json!([{"uid":3,"x":ob.x,"warn":0,"obHit":false}]);
        for (i, dt) in [0., 0.1, 1., 0., 0.1, 3.6, 40., 3.6, 3.6, 3.6, 3.6, 3.6]
            .into_iter()
            .enumerate()
        {
            let riders = match i {
                0 => vec![Rider {
                    id: 0,
                    x: ob.x,
                    y: ob.y,
                }],
                2 => vec![Rider {
                    id: 0,
                    x: ob.x,
                    y: ob.y,
                }],
                3 => vec![Rider {
                    id: 0,
                    x: ob.to,
                    y: ob.y,
                }],
                _ => vec![],
            };
            frames.push(json!({"dt":dt,"spots":riders.iter().map(|r|json!({"r":r.id,"x":r.x,"y":r.y})).collect::<Vec<_>>()}));
            let mut events = vec![];
            ob.update(
                dt,
                &mut World {
                    view: v,
                    mission_done: false,
                    mission_failing: failing,
                    riders: &riders,
                    enemies: &mut enemies,
                    hazards: &mut hazards,
                    strikes: &mut strikes,
                },
                &mut || 0.25,
                &mut events,
            );
            failing |= events.iter().any(|e| matches!(e, Event::Failed(_)));
            states.push(json!({"ob":ob,"enemies":enemies.iter().map(|e|json!([e.x,e.ob_cd])).collect::<Vec<_>>(),"hazards":hazards.iter().map(|h|h.id).collect::<Vec<_>>(),"strikes":strikes.iter().map(|s|s.ob_hit).collect::<Vec<_>>(),"failed":failing}));
        }
        cases.push(json!({"key":format!("{mid}:{stop}"),"enemies":enemy_initial,"hazards":hazard_initial,"strikes":strike_initial,"frames":frames}));
        expected.push(json!({"initial":initial,"states":states}));
    }
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find("const OBJECTIVES = {").unwrap();
    let tail = &source[start..];
    let specs = &tail[..tail.find("\n};").unwrap() + 3];
    let funcs = [
        "obY",
        "makeObjective",
        "dropPiece",
        "hurtObjective",
        "updateObjective",
    ]
    .map(legacy)
    .join("\n");
    let script = format!(
        r#"{specs}
const cases={cases},W={w},H={h},S={s},groundY={gy},topLimit={top},ri=0,reduceMotion=true;
let mission,enemies,hazards,strikes,spots,shake=0,texts=[];const rand=(a,b)=>a+.25*(b-a),clamp=(v,a,b)=>Math.max(a,Math.min(b,v)),dist=(x,y,a,b)=>Math.hypot(x-a,y-b),placeName=p=>p.replace(/^the /,'The '),riderSpots=()=>spots;
const sfx={{thud:()=>{{}},win:()=>{{}},gold:()=>{{}},grr:()=>{{}}}},caption=()=>{{}},plog=()=>{{}},confetti=()=>{{}},toast=()=>{{}},failMission=()=>mission.failT=1.6,RIDERS=[{{name:'test'}}];
{funcs}
const results=cases.map(c=>{{mission={{done:false,failT:0}};enemies=c.enemies;hazards=c.hazards;strikes=c.strikes;makeObjective(OBJECTIVES[c.key]);let initial=JSON.parse(JSON.stringify(mission.ob));let states=c.frames.map(f=>{{spots=f.spots;updateObjective(f.dt);return {{ob:JSON.parse(JSON.stringify(mission.ob)),enemies:enemies.map(e=>[e.x,e.obCd]),hazards:hazards.map(h=>h.uid),strikes:strikes.map(s=>s.obHit),failed:!!mission.failT}};}});return {{initial,states}};}});print(JSON.stringify(results));"#,
        cases = serde_json::to_string(&cases).unwrap(),
        w = v.width,
        h = v.height,
        s = v.scale,
        gy = v.ground_y,
        top = v.top_limit
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
            .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
    compare(&json!(expected), &actual);
}
