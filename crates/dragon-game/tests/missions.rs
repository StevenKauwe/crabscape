use dragon_game::missions::*;
use serde_json::{Value, json};
#[derive(Default)]
struct World {
    s: WorldSnapshot,
    events: Vec<Effect>,
}
impl MissionWorld for World {
    fn snapshot(&self) -> WorldSnapshot {
        self.s.clone()
    }
    fn apply(&mut self, e: Effect) {
        match &e {
            Effect::Spawn { extra: false, .. } => self.s.regular_uncalm += 1,
            Effect::CalmEnemies => self.s.regular_uncalm = 0,
            _ => {}
        }
        self.events.push(e);
    }
}
#[test]
fn authored_stretch_and_stop_bank_order() {
    let defs = missions();
    assert_eq!(defs.len(), 24);
    assert!(defs.iter().all(|d| d.waves.len() == 4));
    assert_eq!(
        defs[0].waves,
        vec![
            vec![("cloud".into(), 6)],
            vec![("cloud".into(), 7)],
            vec![("cloud".into(), 7)],
            vec![("cloud".into(), 8)]
        ]
    );
    let mut m = Mission::new();
    let mut w = World::default();
    m.start_leg(&defs[0], 0, true, &mut w);
    m.arrive_stop(&defs[0], &mut w, &mut || 0.5);
    m.carry = 17.;
    w.events.clear();
    m.wave_complete(&defs[0], &mut w, &mut || 0.5);
    assert_eq!(m.banked, 17.);
    assert_eq!(m.carry, 0.);
    assert_eq!(
        &w.events[..3],
        &[
            Effect::Bank(17.),
            Effect::CalmEnemies,
            Effect::ClearHazardsAndStrikes
        ]
    );
    assert_eq!(m.leg_t, 15.);
    assert_eq!(m.wave, 0);
}
#[test]
fn guest_failure_and_carry_gates_match_browser_order() {
    let def = &missions()[0];
    let mut m = Mission::new();
    let mut w = World::default();
    m.guest = true;
    w.s.player_out = true;
    w.s.stale_world_seconds = 4.;
    m.step(0.1, def, &mut w, &mut || 0.5);
    assert_eq!(m.fail_t, 0.);
    assert!(w.events.contains(&Effect::BecomeHost));
    m.guest = false;
    m.step(0.1, def, &mut w, &mut || 0.5);
    assert_eq!(m.fail_t, 1.6);
    m.step(2., def, &mut w, &mut || 0.5);
    assert!(w.events.contains(&Effect::End(false)));
    m = Mission::new();
    w = World::default();
    m.leg = Leg::Fight;
    m.wave = 0;
    w.s.carry_pending = true;
    m.step(0.1, def, &mut w, &mut || 0.5);
    assert!(m.ob_nag);
    assert_eq!(m.leg, Leg::Fight);
    w.s.carry_pending = false;
    m.step(0.1, def, &mut w, &mut || 0.5);
    assert_eq!(m.leg, Leg::Fly);
}
fn legacy_function(name: &str) -> &str {
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find(&format!("function {name}(")).unwrap();
    let tail = &source[start..];
    &tail[..tail.find("\nfunction ").unwrap()]
}
fn state(m: &Mission) -> Value {
    json!([
        m.wave,
        m.stop,
        m.total,
        m.kills,
        m.spawn_t,
        m.banner_t,
        m.done,
        m.end_t,
        m.fail_t,
        m.t,
        m.carry,
        m.banked,
        m.leg_t,
        m.leg_max,
        m.leg_from,
        m.prog,
        m.trick_t,
        m.ob_nag,
        m.gem_timer,
        m.queue
    ])
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
                compare(a, b)
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_javascript_mission_trace_and_all_definitions() {
    let defs = missions();
    let def = &defs[0];
    let mut expected = vec![];
    let mut m = Mission::new();
    let mut w = World::default();
    m.start_leg(def, 0, true, &mut w);
    let mut frames = vec![];
    for i in 0..180 {
        let dt = [0.1, 0.5, 1.0][i % 3];
        let clear = i % 8 == 7;
        if clear {
            w.s.regular_uncalm = 0;
            m.kills = m.total;
            m.queue.clear();
            m.carry += 3.;
        }
        m.step(dt, def, &mut w, &mut || 0.5);
        frames.push(json!({"dt":dt,"clear":clear}));
        expected.push(state(&m));
    }
    let source = include_str!("../../../web/dragon/index.html");
    let start = source.find("const MISSIONS = [").unwrap();
    let tail = &source[start..];
    let defs_source = &tail[..tail.find("\n];").unwrap() + 3];
    let functions = [
        "startLeg",
        "arriveStop",
        "updateLeg",
        "nextWave",
        "bankGems",
        "waveComplete",
        "updateMission",
        "failMission",
    ]
    .map(legacy_function)
    .join("\n");
    let script = format!(
        r#"
{defs_source}
{stretch}
const defs=MISSIONS.map(m=>({{id:m.id,name:m.name,realm:m.realm,boss:m.boss||null,waves:m.waves}}));
const def=MISSIONS[0], frames={frames};
let mission={{def,wave:-1,stop:0,total:0,kills:0,spawnT:0,bannerT:0,done:false,endT:0,failT:0,t:0,carry:0,banked:0,queue:[],phase:0}},gemTimer=2,enemies=[],gems=[],hazards=[],strikes=[],texts=[],scrollV=0;
const LEG_FIRST=8,LEG_TIME=15,W=1000,H=700,S=20,topLimit=0,P={{x:0,y:0,hearts:5,out:false}},R={{dragon:'test'}},mStats={{}},me={{earned:0}},reduceMotion=true,STATIC=false,time=0;
const ENEMIES={{cloud:{{name:'Storm Cloud'}},bat:{{name:'Storm Bat'}}}};
const rand=(a,b)=>a+.5*(b-a),pick=a=>a[Math.floor(.5*a.length)],clamp=(v,a,b)=>Math.max(a,Math.min(b,v)),skyBottom=()=>600;
const shuffle=a=>{{a=a.slice();for(let i=a.length-1;i>0;i--){{let j=Math.floor(.5*(i+1));[a[i],a[j]]=[a[j],a[i]];}}return a;}};
const caption=()=>{{}},stopOf=()=>['test','test'],stopLine=()=>'',placeName=x=>x,objectiveFor=()=>null,makeObjective=()=>{{}},say=()=>{{}},plog=()=>{{}},markDirty=()=>{{}},confetti=()=>{{}},activeOthers=()=>[],sibKills=()=>0,sibBossDmg=()=>0,updateObjective=()=>{{}},defeat=()=>{{}},becomeHost=()=>{{}},reconcileSkyHost=()=>{{}};
const sfx={{wave:()=>{{}},win:()=>{{}},gold:()=>{{}},wrong:()=>{{}}}};
const spawnEnemy=(type,extra=false)=>enemies.push({{type,extra,calm:false,dead:false}}),spawnGem=()=>gems.push({{x:0}}),endMission=ok=>{{if(ok)bankGems('end');}};
{functions}
startLeg(0,true);
const result=frames.map(f=>{{if(f.clear){{enemies=[];mission.kills=mission.total;mission.queue=[];mission.carry+=3;}}updateMission(f.dt);let m=mission;return [m.wave,m.stop,m.total,m.kills,m.spawnT,m.bannerT,m.done,m.endT,m.failT,m.t,m.carry,m.banked,m.legT,m.legMax,m.legFrom,m.prog,m.trickT,m.obNag,gemTimer,m.queue.slice()];}});
print(JSON.stringify({{defs,result}}));
"#,
        stretch = legacy_function("stretchMission"),
        frames = serde_json::to_string(&frames).unwrap()
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
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    compare(&serde_json::to_value(defs).unwrap(), &value["defs"]);
    for (i, (a, b)) in expected
        .iter()
        .zip(value["result"].as_array().unwrap())
        .enumerate()
    {
        eprintln!("frame {i}: {a} vs {b}");
        compare(a, b);
    }
}

#[test]
fn boss_last_phase_thresholds_and_spawn_draw_order() {
    let defs = missions();
    let def = &defs[3];
    let mut m = Mission::new();
    let mut w = World::default();
    m.start_leg(def, 3, true, &mut w);
    m.arrive_stop(def, &mut w, &mut || 0.5);
    assert_eq!(m.queue.back().map(String::as_str), Some("thunderhead"));
    m.queue.clear();
    w.s.regular_uncalm = 1;
    w.s.boss = Some(BossSnapshot {
        hp: 10.,
        max_hp: 40.,
    });
    m.step(0.1, def, &mut w, &mut || 0.5);
    assert_eq!(m.phase, 2);
    assert_eq!(
        w.events
            .iter()
            .filter(|e| matches!(e, Effect::Spawn { extra: true, .. }))
            .count(),
        6
    );
    assert!(w.events.contains(&Effect::SpeedBoss(1.2)));
    struct ConsumingWorld(World);
    impl MissionWorld for ConsumingWorld {
        fn snapshot(&self) -> WorldSnapshot {
            self.0.snapshot()
        }
        fn apply(&mut self, e: Effect) {
            self.0.apply(e)
        }
        fn apply_random(&mut self, e: Effect, draw: &mut impl FnMut() -> f64) {
            if matches!(e, Effect::Spawn { .. } | Effect::SpawnGem) {
                let _spawn_draw = draw();
            }
            self.apply(e);
        }
    }
    let mut w = ConsumingWorld(World::default());
    m = Mission::new();
    m.leg = Leg::Fight;
    m.wave = 0;
    m.total = 2;
    m.queue.push_back("cloud".into());
    m.spawn_t = 0.;
    let mut draws = [0.1, 0.8].into_iter();
    m.step(0.1, &defs[0], &mut w, &mut || {
        draws.next().expect("exact draw order")
    });
    assert!((m.spawn_t - 1.1).abs() < 1e-12);
    assert_eq!(draws.next(), None);
}
