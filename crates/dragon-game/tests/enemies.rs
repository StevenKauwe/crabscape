use dragon_game::enemies::*;
use serde_json::{Value, json};
fn world() -> World {
    World {
        width: 900.,
        ground_y: 650.,
        top: 50.,
        sky_bottom: 620.,
        scale: 40.,
        player: [450., 300.],
        aim: [450., 300.],
        quiet: false,
        safe: false,
        dragon_scale: 1.,
    }
}
fn enemy(kind: Kind) -> Enemy {
    Enemy::spawn(
        kind,
        7,
        false,
        Some([420., 250.]),
        &world(),
        MissionScaling::default(),
        &mut || 0.37,
    )
}
#[test]
fn strikes_are_telegraphed_and_lunge_starts_next_tick() {
    let mut e = enemy(Kind::Bat);
    e.strike_cd = Some(0.);
    let mut events = vec![];
    e.step(0.1, &world(), &mut || 0.37, &mut events).unwrap();
    assert_eq!(e.pre, 0.4);
    assert!(!e.hurt);
    e.step(0.41, &world(), &mut || 0.37, &mut events).unwrap();
    assert_eq!(e.lunge, 0.42);
    assert!(!e.hurt);
    e.step(0.01, &world(), &mut || 0.37, &mut events).unwrap();
    assert!(e.hurt);
    assert_eq!(e.lunge, 0.41);
}
#[test]
fn wolf_brain_moves_on_ground() {
    let mut e = enemy(Kind::Wolf);
    let before = e.clone();
    e.step(0.1, &world(), &mut || panic!("no draws"), &mut vec![])
        .unwrap();
    assert!(e.target.x > before.target.x);
    assert_eq!(e.target.y, world().ground_y - e.target.r * 0.95);
}
#[test]
fn stun_pauses_brain_but_not_slow_and_calm_exits() {
    let mut e = enemy(Kind::Cloud);
    e.target.stun = 1.;
    e.target.slow = 2.;
    let before = e.clone();
    e.step(0.1, &world(), &mut || panic!("no draws"), &mut vec![])
        .unwrap();
    assert_eq!(e.target.x, before.target.x);
    assert_eq!(e.target.slow, 1.9);
    assert_eq!(e.strike_cd, None);
    e.target.calm = true;
    e.target.x = -200.;
    e.step(0.1, &world(), &mut || 0.37, &mut vec![]).unwrap();
    assert!(e.target.dead);
}
fn snapshot(e: &Enemy) -> Value {
    json!({"x":e.target.x,"y":e.target.y,"hp":e.target.hp,"r":e.target.r,"t":e.t,"wob":e.wob,"dir":e.dir,"spd":e.speed,"cool":e.cool,"cool2":e.cool2,"entered":e.entered,"pre":e.pre,"lunge":e.lunge,"hurt":e.hurt,"strikeCd":e.strike_cd,"baseY":e.base_y,"flash":e.target.flash,"slow":e.target.slow,
    "air":e.air,"vx":e.velocity[0],"vy":e.velocity[1],"windup":e.windup,"sting":e.sting,"risen":e.risen,"cx":e.center.map(|p|p[0]),"cy":e.center.map(|p|p[1]),"split":e.split,"slam":e.slam,"sq":e.squash,"retarget":e.retarget,"tx":e.destination.map(|p|p[0]),"ty":e.destination.map(|p|p[1])})
}
fn attack_snapshot(events: &[Event]) -> Value {
    let mut hazards = vec![];
    let mut strikes = vec![];
    for event in events {
        match event {
            Event::Hazard(h)=>hazards.push(json!({"kind":h.kind,"x":h.position[0],"y":h.position[1],"vx":h.velocity[0],"vy":h.velocity[1],"g":h.gravity,"r":h.radius,"life":h.life,"warn":h.warn})),
            Event::Spark{position,velocity,radius,life}=>hazards.push(json!({"kind":"spark","x":position[0],"y":position[1],"vx":velocity[0],"vy":velocity[1],"g":0.,"r":radius,"life":life,"warn":0.})),
            Event::Strike{x,y0,warn,bolt}=>strikes.push(json!({"x":x,"y0":y0,"warn":warn,"bolt":bolt})),
            _=>{}
        }
    }
    json!({"hazards":hazards,"strikes":strikes})
}
fn block(start: &str, end: &str) -> String {
    let src = include_str!("../../../web/dragon/index.html");
    let i = src.find(start).unwrap();
    let rest = &src[i..];
    let j = rest.find(end).unwrap();
    rest[..j].to_string()
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let x = x.as_f64().unwrap();
            let y = y.as_f64().unwrap();
            assert!((x - y).abs() < 1e-9 * (1. + x.abs()), "{a} != {b}");
        }
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                compare(v, &y[k]);
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len());
            for (a, b) in x.iter().zip(y) {
                compare(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_source_spawn_and_600_frames_match_supported_brains() {
    let kinds = [
        Kind::Cloud,
        Kind::Bigcloud,
        Kind::Bat,
        Kind::Sprite,
        Kind::Thunderhead,
        Kind::Urgal,
        Kind::Wolf,
        Kind::Troll,
        Kind::Chieftain,
        Kind::Cavetroll,
        Kind::Jelly,
        Kind::Leaper,
        Kind::Serpent,
        Kind::Slime,
        Kind::Slimelet,
        Kind::Imp,
        Kind::Bigslime,
        Kind::Gobbler,
        Kind::Moth,
        Kind::Swallower,
    ];
    let mut expected = vec![];
    for kind in kinds {
        let mut draw_index = 0usize;
        let mut draw = || {
            let value = [0.13, 0.77, 0.26, 0.95, 0.41][draw_index % 5];
            draw_index += 1;
            value
        };
        let mut e = Enemy::spawn(
            kind,
            7,
            false,
            Some([420., 250.]),
            &world(),
            MissionScaling::default(),
            &mut draw,
        );
        for i in 0..600 {
            let mut w = world();
            w.aim = [
                450. + (i as f64 * 0.031).sin() * 250.,
                300. + (i as f64 * 0.023).cos() * 120.,
            ];
            w.quiet = (200..260).contains(&i);
            if i == 300 {
                e.target.hp = e.target.max_hp * 0.35;
            }
            let mut events = vec![];
            e.step(1. / 30., &w, &mut draw, &mut events).unwrap();
            expected.push(json!({"state":snapshot(&e),"attacks":attack_snapshot(&events)}));
        }
    }
    let script = format!(
        r#"
{defs}
const S=40,W=900,groundY=650,topLimit=50,BOSS_HP=1.35;
let mission=null,P={{x:450,y:300,sila:0}},A, enemies=[],strikes=[],hazards=[],rings=[],texts=[],gems=[],shake=0;
const reduceMotion=true,poofAt=()=>{{}},burst=()=>{{}},sibBossDmg=()=>0;
let drawIndex=0;const draw=()=>[0.13,0.77,0.26,0.95,0.41][drawIndex++%5];
const rand=(a,b)=>a+draw()*(b-a);Math.random=draw;
const skyBottom=()=>620,skyToughness=()=>1,scaleOf=()=>1,bonded=()=>({{}}),retarget=()=>{{}},aimFor=()=>A;
const clamp=(v,a,b)=>Math.max(a,Math.min(b,v)),dist=(a,b,c,d)=>Math.hypot(a-c,b-d),isSafe=()=>false,touchHit=()=>{{}},bonk=()=>{{}};
const sfx={{throw:()=>{{}},zap:()=>{{}},smash:()=>{{}}}},nextUid=()=>1;
{make}
// World spawn application is the event consumer's responsibility on both sides.
function spawnEnemy(type,extra,at){{return {{}};}}
{helpers}
{update}
const results=[];
for(const kind of {kinds}){{drawIndex=0;let e=makeEnemy(kind,false,{{x:420,y:250}});enemies=[e];
 for(let i=0;i<600;i++){{A={{x:450+Math.sin(i*0.031)*250,y:300+Math.cos(i*0.023)*120,me:true}};P.sila=i>=200&&i<260?1:0;if(i===300)e.hp=e.maxHp*0.35;hazards=[];strikes=[];updateEnemies(1/30);
 const out={{}};for(const k of ['x','y','hp','r','t','wob','dir','spd','cool','cool2','entered','flash','slow'])out[k]=e[k];
 for(const k of ['pre','lunge','vx','vy','windup','sting','sq','retarget'])out[k]=e[k]||0;
 for(const k of ['hurt','air','risen','slam'])out[k]=!!e[k];
 for(const k of ['strikeCd','baseY','cx','cy','split','tx','ty'])out[k]=e[k]??null;
 results.push({{state:out,attacks:{{hazards:hazards.map(h=>({{kind:h.kind,x:h.x,y:h.y,vx:h.vx,vy:h.vy,g:h.g||0,r:h.r,life:h.life,warn:h.warn||0}})),strikes:strikes.map(h=>({{x:h.x,y0:h.y0??null,warn:h.warn,bolt:h.bolt}}))}}}});
 }}
}}
print(JSON.stringify(results));
"#,
        defs = block("const ENEMIES = {", "\n};") + "\n};",
        make = block("function makeEnemy(", "\nfunction "),
        helpers = block("function throwAt(", "\nfunction updateEnemies("),
        kinds = serde_json::to_string(&kinds).unwrap(),
        update = block("function updateEnemies(", "\nfunction ")
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
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), actual.len());
    for (a, b) in expected.iter().zip(&actual) {
        compare(a, b);
    }
}

#[test]
fn cloud_strike_has_source_origin_and_boss_adds_ignore_quiet() {
    let mut e = enemy(Kind::Cloud);
    e.pre = 0.01;
    let mut events = vec![];
    e.step(0.02, &world(), &mut || 0.37, &mut events).unwrap();
    assert!(events.contains(&Event::Strike {
        x: e.target.x,
        y0: Some(e.target.y + e.target.r * 0.5),
        warn: 0.45,
        bolt: 0.25
    }));
    let mut boss = enemy(Kind::Thunderhead);
    boss.cool = 0.;
    boss.cool2 = 0.;
    let mut w = world();
    w.quiet = true;
    events.clear();
    boss.step(
        0.1,
        &w,
        &mut || panic!("boss does not draw here"),
        &mut events,
    )
    .unwrap();
    assert_eq!(
        events,
        vec![
            Event::Spawn {
                kind: Kind::Cloud,
                extra: true,
                at: [
                    boss.target.x - boss.target.r,
                    boss.target.y + boss.target.r * 0.8
                ]
            },
            Event::Spawn {
                kind: Kind::Cloud,
                extra: true,
                at: [
                    boss.target.x + boss.target.r,
                    boss.target.y + boss.target.r * 0.8
                ]
            },
        ]
    );
    assert_eq!(boss.cool, 2.6);
    assert_eq!(boss.cool2, 7.);
}

#[test]
fn spawn_uses_source_draw_order_and_boss_health_multiplier() {
    let mut draws = 0;
    let e = Enemy::spawn(
        Kind::Bat,
        1,
        false,
        None,
        &world(),
        MissionScaling {
            difficulty: 2.,
            toughness: 1.5,
            sky_toughness: 1.2,
            speed: 1.3,
        },
        &mut || {
            draws += 1;
            0.25
        },
    );
    assert_eq!(draws, 7);
    assert_eq!(e.dir, 1.);
    assert_eq!(e.target.x, -16.8);
    assert!((e.target.hp - 2.16).abs() < 1e-12);
    assert!((e.speed - 195.52).abs() < 1e-12);
    let boss = enemy(Kind::Thunderhead);
    assert_eq!(boss.target.hp, 54.);
    assert!(boss.entered);
}

#[test]
fn gobbler_selects_first_nearest_gem_and_uses_pre_move_distance() {
    let mut e = enemy(Kind::Gobbler);
    e.strike_cd = Some(100.);
    let gems = [
        Gem {
            id: 11,
            position: [430., 250.],
            radius: 3.,
        },
        Gem {
            id: 22,
            position: [410., 250.],
            radius: 3.,
        },
    ];
    let mut events = vec![];
    e.step_with_context(
        0.1,
        &world(),
        Context {
            gems: &gems,
            ..Context::default()
        },
        &mut || 0.37,
        &mut events,
    )
    .unwrap();
    assert!(e.target.x > 420.);
    assert!(events.contains(&Event::Gobble { gem_id: 11 }));
    assert!(!events.contains(&Event::Gobble { gem_id: 22 }));
}

#[test]
fn quiet_does_not_cancel_already_wound_ground_throw() {
    let mut e = enemy(Kind::Chieftain);
    e.windup = 0.01;
    let mut w = world();
    w.quiet = true;
    let mut events = vec![];
    e.step(0.02, &w, &mut || 0.37, &mut events).unwrap();
    let h: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Hazard(h) = e {
                Some(h)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(h.len(), 3);
    for (h, off) in h.iter().zip([-1.4, 0., 1.4]) {
        assert_eq!(h.kind, HazardKind::Rock);
        let x = h.position[0] + h.velocity[0];
        let y = h.position[1] + h.velocity[1] + h.gravity * 0.5;
        assert!((x - (w.aim[0] + off * w.scale)).abs() < 1e-10);
        assert!((y - w.aim[1]).abs() < 1e-10);
    }
}

#[test]
fn boss_add_caps_and_split_threshold_use_context() {
    let mut e = enemy(Kind::Bigslime);
    e.target.hp = e.target.max_hp;
    let mut events = vec![];
    e.step_with_context(
        0.,
        &world(),
        Context {
            sibling_boss_damage: e.target.max_hp * 0.3,
            active_extras: 5,
            gems: &[],
        },
        &mut || 0.37,
        &mut events,
    )
    .unwrap();
    assert!((e.split.unwrap() - 0.6).abs() < 1e-12);
    assert!(!events.iter().any(|e| matches!(e, Event::SpawnBaby { .. })));
    e.target.hp = e.target.max_hp * 0.2;
    e.step_with_context(0., &world(), Context::default(), &mut || 0.37, &mut events)
        .unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::SpawnBaby { .. }))
            .count(),
        2
    );
    let mut chief = enemy(Kind::Chieftain);
    chief.cool2 = 0.;
    events.clear();
    chief
        .step_with_context(
            0.,
            &world(),
            Context {
                active_extras: 4,
                ..Context::default()
            },
            &mut || 0.37,
            &mut events,
        )
        .unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Spawn { .. })));
}

#[test]
fn leaper_underwater_is_not_touchable_and_inhale_is_separate() {
    let mut e = enemy(Kind::Leaper);
    let mut w = world();
    w.player = [e.target.x, w.ground_y + e.target.r * 1.1];
    let mut events = vec![];
    e.step(0., &w, &mut || 0.37, &mut events).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Touch { .. })));
    let mut boss = enemy(Kind::Swallower);
    boss.windup = 1.;
    let mouth = [
        boss.target.x + boss.dir * boss.target.r * 0.45,
        boss.target.y + boss.target.r * 0.25,
    ];
    let player = [mouth[0] - 1., mouth[1]];
    assert_eq!(inhale_displacement(&boss, player, false, 40., 1.), [1., 0.]);
    assert_eq!(inhale_displacement(&boss, player, true, 40., 1.), [0., 0.]);
}
