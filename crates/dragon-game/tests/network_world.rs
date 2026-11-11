#[cfg(target_os = "macos")]
use dragon_game::missions::Leg;
use dragon_game::{
    authority::Authority,
    enemies::{Enemy, Gem, Kind, MissionScaling, World},
    network_world::*,
};
use serde_json::{Value, json};
fn view() -> Viewport {
    Viewport {
        width: 1000.,
        top: 50.,
        ground: 650.,
        scale: 20.,
    }
}
fn guest() -> GuestWorld {
    let mut w = GuestWorld::new(
        "m3".into(),
        4,
        Authority {
            guest: true,
            host_peer: Some("host".into()),
            host_rider: 1,
            epoch: 2,
        },
    );
    w.run = "run".into();
    w
}
fn spawn(kind: Kind, at: [f64; 2]) -> Enemy {
    let v = view();
    let w = World {
        width: v.width,
        ground_y: v.ground,
        top: v.top,
        sky_bottom: 600.,
        scale: v.scale,
        player: [500., 300.],
        aim: [500., 300.],
        quiet: false,
        safe: false,
        dragon_scale: 1.,
    };
    let mut e = Enemy::spawn(
        kind,
        0,
        false,
        Some(at),
        &w,
        MissionScaling::default(),
        &mut || 0.5,
    );
    e.target.max_hp = 10.;
    e.target.hp = 10.;
    e.target.flash = 0.;
    e.air = false;
    e.t = 0.;
    e.sting = 0.;
    e.squash = 0.;
    e
}
fn packet() -> Value {
    json!({"r":1,"mid":"m3","run":"run","epoch":2,"next":8,"wave":0,"lg":0,"sp":0,"pr":321,"total":4,"kills":1,"q":["cloud","invalid","bat"],"ob":null,"en":[[1,0,0.1235,0.5,75,-1,0]],"hz":[[2,4,0.2,0.3,0.1,-0.2,0.5]],"st":[[3,0.4,0.2,0.25,-1]],"done":0})
}
#[test]
fn sender_run_epoch_admission_is_atomic() {
    for change in 0..6 {
        let mut w = guest();
        let mut d = packet();
        let mut peer = "host";
        match change {
            0 => peer = "forged",
            1 => d["run"] = json!("other"),
            2 => d["epoch"] = json!(1),
            3 => d["epoch"] = json!(2.5),
            4 => d["mid"] = json!("m2"),
            _ => w.authority.guest = false,
        };
        assert!(w.receive(&d, peer, view(), 10., &mut spawn).is_none());
        assert_eq!(w.next_uid, 0);
        assert_eq!(w.last_world, 0.);
        assert!(w.enemies.is_empty());
    }
}
#[test]
fn bounded_projection_used_attacks_and_once_only_results() {
    let mut w = guest();
    w.used_hazards.insert(2);
    w.used_strikes.insert(3);
    let mut d = packet();
    d["en"] = json!(
        (0..50)
            .map(|i| json!([i, 0, -9, 9, 900, 0, 511]))
            .collect::<Vec<_>>()
    );
    d["st"] = json!((0..20).map(|i| json!([i, 9, 8, 7, 9])).collect::<Vec<_>>());
    d["done"] = json!(1);
    d["lg"] = json!(1);
    d["sp"] = json!(2);
    let events = w.receive(&d, "host", view(), 10., &mut spawn).unwrap();
    assert_eq!(w.enemies.len(), 40);
    assert_eq!(w.strikes.len(), 10);
    assert!(w.hazards.is_empty());
    assert!(w.strikes[3].hit);
    assert_eq!(w.enemies[0].destination, [-300., 830.]);
    assert_eq!(w.enemies[0].enemy.target.hp, 10.);
    assert!(events.contains(&Event::Bank(BankReason::Stop)));
    assert!(events.contains(&Event::Bank(BankReason::End)));
    assert!(w.mission.done);
    assert_eq!(w.mission.end_t, 3.4);
    assert!(
        w.receive(&d, "host", view(), 11., &mut spawn)
            .unwrap()
            .is_empty()
    );
}
#[derive(Default)]
struct Context {
    safe: bool,
    gems: Vec<Gem>,
    events: Vec<MirrorEvent>,
}
impl MirrorWorld for Context {
    fn safe(&self) -> bool {
        self.safe
    }
    fn player(&self) -> [f64; 2] {
        [100., 100.]
    }
    fn gems(&self) -> &[Gem] {
        &self.gems
    }
    fn apply(&mut self, e: MirrorEvent) {
        match e {
            MirrorEvent::Touch { .. } | MirrorEvent::Sting { .. } => self.safe = true,
            MirrorEvent::Gobble { gem, .. } => self.gems.retain(|g| g.id != gem),
        }
        self.events.push(e);
    }
}
#[test]
fn mirror_callbacks_are_synchronous_and_no_host_brain_runs() {
    let mut e = spawn(Kind::Gobbler, [100., 100.]);
    e.target.x = 100.;
    e.target.y = 100.;
    e.target.id = 1;
    e.sting = 0.4;
    e.hurt = true;
    let mut mirrors = vec![
        Mirror {
            enemy: e.clone(),
            destination: [100., 100.],
        },
        Mirror {
            enemy: e,
            destination: [100., 100.],
        },
    ];
    let mut world = Context {
        gems: vec![Gem {
            id: 7,
            position: [100., 100.],
            radius: 2.,
        }],
        ..Context::default()
    };
    update_mirrors(&mut mirrors, 0.01, view(), 1., &mut world);
    assert_eq!(
        world.events,
        vec![
            MirrorEvent::Gobble { enemy: 1, gem: 7 },
            MirrorEvent::Touch {
                enemy: 1,
                hurts: true
            }
        ]
    );
    assert_eq!(mirrors[0].enemy.sting, 0.39);
    assert!((mirrors[0].enemy.cool - 2.3).abs() < 1e-9);
}
#[cfg(target_os = "macos")]
fn projection(w: &GuestWorld, events: &[Event]) -> Value {
    let m = &w.mission;
    let ev = events
        .iter()
        .map(|e| match e {
            Event::Sound(s) => match s {
                Sound::Wave => "wave".into(),
                Sound::Wrong => "wrong".into(),
                Sound::Win => "win".into(),
            },
            Event::Encouragement => "say:encourage".into(),
            Event::Arrival(s) => format!("arrival:{s}"),
            Event::Travel { from, to } => format!("travel:{from}:{to}"),
            Event::Bank(BankReason::Stop) => "bank:stop".into(),
            Event::Bank(BankReason::End) => "bank:end".into(),
            Event::FailureCaption(s) => format!("failure:{s}"),
            Event::Complete => "complete".into(),
        })
        .collect::<Vec<String>>();
    json!({"epoch":w.authority.epoch,"rider":w.authority.host_rider,"next":w.next_uid,"time":w.last_world,"run":w.run,"wave":m.wave,"banner":w.banner,"bannerT":m.banner_t,"leg":if m.leg==Leg::Fly{"fly"}else{"fight"},"stop":m.stop,"prog":m.prog,"total":m.total,"kills":m.kills,"queue":w.last_queue,"done":m.done,"endT":m.end_t,"failT":m.fail_t,"fw":w.fail_why,"events":ev,
    "ob":w.objective.as_ref().map(|o|json!([o.what,o.x,o.y,o.hp,o.max_hp,o.carrier,o.done,o.to,o.t,o.hurt_t,o.r])),
    "en":w.enemies.iter().map(|m|{let e=&m.enemy;let t=&e.target;json!([t.id,e.kind,t.x,t.y,m.destination[0],m.destination[1],t.hp,e.dir,t.calm,t.flash,t.slow,e.windup,e.pre,e.air,t.stun,e.hurt,t.cracked,t.marked,e.squash])}).collect::<Vec<_>>(),
    "hz":w.hazards.iter().map(|h|{let a=&h.hazard;json!([h.uid,a.kind,a.position[0],a.position[1],a.velocity[0],a.velocity[1],a.gravity,a.radius,a.life,a.warn,h.spin])}).collect::<Vec<_>>(),
    "st":w.strikes.iter().map(|s|json!([s.uid,s.x,s.warn,s.bolt,s.hit,s.y0])).collect::<Vec<_>>()})
}
#[cfg(target_os = "macos")]
fn js(script: &str) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut c =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
            .args(["-e", "eval(JSON.parse(readline()))"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", json!(script)).as_bytes())
        .unwrap();
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[cfg(target_os = "macos")]
fn equivalent(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equivalent(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: {a:?} != {b:?}");
            for (k, a) in a {
                equivalent(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires system JavaScriptCore; evaluates the actual source onWorld"]
fn actual_js_receive_matches_flags_bounds_events_and_repeated_snapshots() {
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find("function onWorld(").unwrap();
    let end = start + source[start..].find("// One authority").unwrap();
    let function = &source[start..end];
    let mut cases = Vec::new();
    for flags in 0..512 {
        let mut d = packet();
        d["en"][0][6] = json!(flags);
        d["ob"] = json!([2, 5, 0.25, 0.4, 4, 6, 1, 0, 0.8]);
        cases.push(d);
    }
    for done in [0, 2, 1, 1] {
        let mut d = packet();
        d["done"] = json!(done);
        d["fw"] = json!("Lost the objective");
        d["wave"] = json!(2);
        d["sp"] = json!(2);
        d["lg"] = json!(1);
        d["total"] = json!(900);
        d["kills"] = json!(-2);
        d["pr"] = json!(1200);
        d["ob"] = json!([0, 4, -3, 5, 3, 40, -1, 1, 4]);
        cases.push(d);
    }
    let mut d = packet();
    d["en"] = json!(
        (0..45)
            .map(|i| json!([i, i % 20, -2, 4, -8, 1, 0]))
            .collect::<Vec<_>>()
    );
    d["hz"] = json!(
        (0..45)
            .map(|i| json!([i, i % 9, -2, 4, 0.12, -0.23, 9]))
            .collect::<Vec<_>>()
    );
    d["st"] = json!((0..14).map(|i| json!([i, 3, 4, 5, 2])).collect::<Vec<_>>());
    cases.push(d);
    let mut w = guest();
    w.used_hazards.insert(2);
    w.used_strikes.insert(3);
    let expected = cases
        .iter()
        .map(|d| {
            let events = w.receive(d, "host", view(), 42., &mut spawn).unwrap();
            projection(&w, &events)
        })
        .collect::<Vec<_>>();
    let script = format!(
        r#"
const STATIC=true,W=1000,topLimit=50,groundY=650,S=20,time=42,reduceMotion=true,P={{x:100,y:100}},R={{dragon:'D'}};
const TYPE_LIST={types},HAZ_KINDS=['rock','boulder','spark','fall','bubble','ember','dust'],HAZ_SIZE={{rock:[.2,9],boulder:[.34,9],spark:[.14,0],fall:[.28,8],bubble:[.24,-.5],ember:[.2,9],dust:[.26,1.2]}},OB_KINDS=['protect','escort','carry'],OB_WHATS=['barn','fawn','goat','lamp','boats','star','garden'];
const ENEMIES=Object.fromEntries(TYPE_LIST.map(x=>[x,{{}}])),OBJECTIVES={{}},usedHaz=new Set([2]),usedStrike=new Set([3]);
let uidN=0,enemies=[],hazards=[],strikes=[],events=[];
let mission={{guest:true,hostPeer:'host',hostRider:1,authority:2,run:'run',def:{{id:'m3',waves:[[],[],[],[]]}},wave:-1,leg:'fly',stop:0,prog:0,total:0,kills:0,done:false,endT:0,failT:0,banner:'',bannerT:0,lastQueue:[],failWhy:''}};
const clamp=(x,a,b)=>Math.max(a,Math.min(b,x));const pick=a=>a[0];const sfx={{wave:()=>events.push('wave'),wrong:()=>events.push('wrong'),win:()=>events.push('win')}};function say(s){{if(s!=='We did it!')events.push('say:encourage');}}function bankGems(s){{events.push('bank:'+s);}}function confetti(){{events.push('complete');}}function stopLine(d,s){{return 'arrival:'+s;}}function stopOf(d,s){{return [s];}}function placeName(s){{return s;}}function caption(s){{if(s.includes(' is safe!')){{const n=mission.stop; events.push('travel:'+((+s.split(' ')[0]))+':'+(+s.split('On to ')[1].split('!')[0]));}}else if(s.startsWith('arrival:'))events.push(s);else events.push('failure:'+s);}}
const radii={radii};function makeEnemy(type,extra,p){{return {{type,x:p.x,y:p.y,hp:10,maxHp:10,r:radii[TYPE_LIST.indexOf(type)]*20,t:0,flash:0,air:false,sq:0}};}}
{function}
function project(){{const M=mission;return {{epoch:M.authority,rider:M.hostRider,next:uidN,time:M.lastWorld,run:M.run,wave:M.wave,banner:M.banner,bannerT:M.bannerT,leg:M.leg,stop:M.stop,prog:M.prog,total:M.total,kills:M.kills,queue:M.lastQueue,done:M.done,endT:M.endT,failT:M.failT,fw:M.failWhy,events,ob:M.ob?[M.ob.what,M.ob.x,M.ob.y,M.ob.hp,M.ob.maxHp,M.ob.carrier,M.ob.done,M.ob.to,M.ob.t,M.ob.hurtT,M.ob.r]:null,en:enemies.map(e=>[e.uid,e.type,e.x,e.y,e.mx,e.my,e.hp,e.dir,e.calm,e.flash,e.slow,e.windup,e.pre,e.air,e.stun,e.hurt,e.cracked,e.marked,e.sq]),hz:hazards.map(h=>[h.uid,h.kind,h.x,h.y,h.vx,h.vy,h.g,h.r,h.life,h.warn,h.spin]),st:strikes.map(s=>[s.uid,s.x,s.warn,s.bolt,s.hit,s.y0??null])}};}}
const cases={cases};print(JSON.stringify(cases.map(d=>{{events=[];onWorld(d,'host');return project();}})));
"#,
        types = json!(TYPES),
        radii = json!(TYPES.map(|k| k.definition().radius)),
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "receive");
}
#[test]
fn malformed_indices_and_ids_are_deliberately_not_coerced() {
    for bad in [Value::Null, json!(false), json!([0]), json!({}), json!("0")] {
        let mut w = guest();
        let mut d = packet();
        d["en"][0][1] = bad.clone();
        d["ob"] = json!([bad, 0, 0, 0, 1, 1, -1, 0, 0]);
        w.receive(&d, "host", view(), 1., &mut spawn).unwrap();
        assert!(w.enemies.is_empty());
        assert!(w.objective.is_none());
    }
    for bad in [
        json!(-1),
        json!(1.5),
        json!("1"),
        json!(9007199254740992_u64),
    ] {
        let mut w = guest();
        let mut d = packet();
        d["en"][0][0] = bad;
        w.receive(&d, "host", view(), 1., &mut spawn).unwrap();
        assert!(w.enemies.is_empty());
    }
}
#[cfg(target_os = "macos")]
fn source_enemy(e: &Enemy) -> Value {
    let t = &e.target;
    json!({"uid":t.id,"type":e.kind,"x":t.x,"y":t.y,"r":t.r,"hp":t.hp,"maxHp":t.max_hp,"dir":e.dir,"calm":t.calm,"flash":t.flash,"slow":t.slow,"windup":e.windup,"pre":e.pre,"air":e.air,"stun":t.stun,"hurt":e.hurt,"cracked":t.cracked,"marked":t.marked,"dead":t.dead,"sting":e.sting,"t":e.t,"def":{"ground":e.kind.definition().ground,"hop":e.kind.definition().hop}})
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires system JavaScriptCore; evaluates actual shareWorld and updateMirrors"]
fn actual_js_encoder_and_mirror_interpolation_match() {
    let source = include_str!("../../../web/dragon/index.html");
    let extract = |a: &str, b: &str| {
        let start = source.find(a).unwrap();
        let end = start + source[start..].find(b).unwrap();
        &source[start..end]
    };
    let share = extract("function shareWorld()", "function onWorld(");
    let mirrors = extract("function updateMirrors(", "// If the host's iPad");
    let hurt = extract("const hurtsNow =", "const touchHit =");
    let sting = extract("function stingCheck(", "function groundAttack(");
    let touch = extract("const touchable =", "function spawnBaby(");
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    for flags in 0..512 {
        let mut w = guest();
        let mut d = packet();
        d["en"][0][6] = json!(flags);
        w.receive(&d, "host", view(), 1., &mut spawn).unwrap();
        w.mission.queue = w.last_queue.iter().cloned().collect();
        w.enemies[0].enemy.target.x = -0.5;
        w.enemies[0].enemy.target.y = 49.7;
        let m = &w.mission;
        inputs.push(json!({"mission":{"def":{"id":w.mission_id},"guest":false,"run":w.run,"authority":w.authority.epoch,"wave":m.wave,"banner":w.banner,"leg":"fight","stop":m.stop,"prog":m.prog,"total":m.total,"kills":m.kills,"done":m.done,"failT":m.fail_t,"queue":m.queue,"failWhy":w.fail_why,"ob":null},"next":w.next_uid,"enemies":w.enemies.iter().map(|e|source_enemy(&e.enemy)).collect::<Vec<_>>(),"hazards":w.hazards.iter().map(|h|json!({"uid":h.uid,"kind":h.hazard.kind,"x":h.hazard.position[0],"y":h.hazard.position[1],"vx":h.hazard.velocity[0],"vy":h.hazard.velocity[1],"warn":h.hazard.warn})).collect::<Vec<_>>(),"strikes":w.strikes.iter().map(|s|json!({"uid":s.uid,"x":s.x,"warn":s.warn,"bolt":s.bolt})).collect::<Vec<_>>() }));
        expected.push(encode_world(&w, 1, view()));
    }
    let script = format!(
        r#"const W=1000,topLimit=50,groundY=650,ri=1,time=42,TYPE_LIST={types},HAZ_KINDS=['rock','boulder','spark','fall','bubble','ember','dust'],OB_KINDS=['protect','escort','carry'],OB_WHATS=['barn','fawn','goat','lamp','boats','star','garden'];let mission,enemies,hazards,strikes,worldT,uidN,out;const clamp=(x,a,b)=>Math.max(a,Math.min(b,x)),liveOn=()=>true,othersInMission=()=>true,room={{emit:(topic,d)=>{{out=d;return {{catch:()=>{{}}}};}}}};{hurt}{share}const cases={inputs};print(JSON.stringify(cases.map(c=>{{mission=c.mission;enemies=c.enemies;hazards=c.hazards;strikes=c.strikes;uidN=c.next;worldT=0;shareWorld();return out;}})));"#,
        types = json!(TYPES),
        inputs = json!(inputs)
    );
    equivalent(&json!(expected), &js(&script), "encode");
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    for kind in TYPES {
        for air in [false, true] {
            let mut e = spawn(kind, [100., 100.]);
            e.target.id = 1;
            e.target.x = 100.;
            e.target.y = 100.;
            e.target.flash = 0.1;
            e.air = air;
            e.sting = 0.4;
            e.hurt = true;
            let mut input = source_enemy(&e);
            input["mx"] = json!(101.);
            input["my"] = json!(102.);
            inputs.push(input);
            let mut m = vec![Mirror {
                enemy: e,
                destination: [101., 102.],
            }];
            let mut c = Context::default();
            update_mirrors(&mut m, 0.01, view(), 1., &mut c);
            let e = &m[0].enemy;
            expected.push(json!([
                e.t,
                e.target.flash,
                e.target.x,
                e.target.y,
                e.sting,
                c.events
                    .iter()
                    .map(|e| match e {
                        MirrorEvent::Touch { hurts, .. } =>
                            if *hurts {
                                "bonk"
                            } else {
                                "bump"
                            },
                        MirrorEvent::Sting { .. } => "bonk",
                        _ => "gobble",
                    })
                    .collect::<Vec<_>>()
            ]));
        }
    }
    let script = format!(
        r#"const S=20,groundY=650,P={{x:100,y:100}},gems=[],scaleOf=()=>1,bonded=()=>null,dist=(a,b,c,d)=>Math.hypot(a-c,b-d);let enemies,safe,events;const isSafe=()=>safe;function bonk(){{events.push('bonk');safe=true;}}function bump(){{events.push('bump');safe=true;}}function gobbleGem(){{events.push('gobble');}}{hurt}{touch}{sting}const touchHit=e=>{{if(hurtsNow(e))bonk();else bump();}};{mirrors}const cases={inputs};print(JSON.stringify(cases.map(e=>{{enemies=[e];safe=false;events=[];updateMirrors(.01);return [e.t,e.flash,e.x,e.y,e.sting,events];}})));"#,
        inputs = json!(inputs)
    );
    equivalent(&json!(expected), &js(&script), "mirrors");
}
