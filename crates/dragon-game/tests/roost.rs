use dragon_game::{
    profile,
    random::Mulberry32,
    roost::{self, Error, Event, Roost},
};
use serde_json::{Value, json};
fn normalized(raw: Value) -> Value {
    profile::normalize_profile(&raw, "rider3", 2, || "di".into()).unwrap()
}
fn cases() -> Vec<Value> {
    let mut out = Vec::new();
    for full in [false, true] {
        for golden in [false, true] {
            for reduce in [false, true] {
                for mode in [0, 1, 2] {
                    let mut me = normalized(
                        json!({"xpv":2,"eggs":2,"eggAwake":true,"goldenPending":golden,"hatched":11,"lastRealm":"sea","party":[{"bonded":true,"xp":0},{"id":"old","kind":"flame","name":"Sparky"}],"main":"old","kindsSeen":if mode==2{json!(["flame","spark","boulder","frost"])}else{json!(["spark"])}}),
                    );
                    if !full {
                        me["cleared"] = json!(["m1", "m2", "m3"]);
                    }
                    me["wild"]=json!((0..40).map(|i|json!({"name":format!("Wild {i}"),"kind":"frost","color":"#6cc6f0","realm":"wood","from":2})).collect::<Vec<_>>());
                    let other = normalized(
                        json!({"hatched":9,"party":[{"bonded":true,"xp":0},{"id":"p","kind":"frost","name":"Ember"}]}),
                    );
                    out.push(json!({"me":me,"others":[other],"static":mode==2,"reduce":reduce,"releaseBaby":mode==1,"seed":42+mode,"name":if mode==2{"!!!"}else{"José 龙"}}));
                }
            }
        }
    }
    out
}
fn block(start: &str, end: &str, inclusive: bool) -> &'static str {
    let s = include_str!("../../../web/dragon/index.html");
    let t = &s[s.find(start).unwrap()..];
    &t[..t.find(end).unwrap() + if inclusive { end.len() } else { 0 }]
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{a} != {b}"
        ),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn full_party_validation_is_atomic_and_golden_award_is_once() {
    let mut me = cases()[0]["me"].clone();
    me["cleared"] = json!([]);
    let mut r = Roost::default();
    r.begin_hatch(&me, &[], true, 1234, true, || 0.5).unwrap();
    let before = me.clone();
    let baby = r.pending_baby.clone();
    assert_eq!(
        r.finish_hatch(&mut me, "Baby", None, 2, 1235),
        Err(Error::InvalidRelease)
    );
    assert_eq!(me, before);
    assert_eq!(r.pending_baby, baby);
    me["hatched"] = json!(12);
    r.check_milestones(&mut me, &[], true, true, true).unwrap();
    assert_eq!(me["eggs"], 3.0);
    assert!(roost::egg_ready(&me));
    r.check_milestones(&mut me, &[], true, true, true).unwrap();
    assert_eq!(me["eggs"], 3.0);
    assert_eq!(r.milestone_queue.len(), 5);
    let e = r.close_milestone(&mut me, true).unwrap();
    assert_eq!(me["seenMs"], json!(["first"]));
    assert_eq!(
        e,
        vec![Event::Dirty, Event::ShowMilestone { id: "song".into() }]
    );
}
#[test]
fn release_uses_index_not_duplicate_id_and_preserves_bond() {
    let mut me = normalized(
        json!({"party":[{"bonded":true},{"id":"same","name":"One","kind":"flame"},{"id":"same","name":"Two","kind":"frost"}],"main":"same"}),
    );
    let r = Roost::default();
    assert_eq!(
        r.set_free(&mut me, 0, 2, 1, false),
        Err(Error::InvalidRelease)
    );
    r.set_free(&mut me, 2, 2, 1, false).unwrap();
    assert_eq!(me["party"][1]["name"], "One");
    assert_eq!(me["main"], "bond");
    assert_eq!(me["wild"][0]["name"], "Two");
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_hatch_release_milestones_and_rng_match() {
    let mut script = String::from(
        r#"let me,others,STATIC,reduceMotion,pendingBaby,state,time=0,hatchAt,msQueue,events,draws;const ri=2,W=800,H=600,S=40,TAU=Math.PI*2;let particles=[];const pressed={clear(){}};const P={followCool:{}};const wingState={delete(id){events.push({event:'forget_follower',dragon:id})}};
const nodes={};const node=()=>({hidden:true,value:'',disabled:false,appendChild(){},addEventListener(){}});const $=id=>nodes[id]||(nodes[id]=node());let chosen=null;
const el=node;const document={createElement:node,createTextNode:()=>({}),querySelector:()=>chosen===null?null:{value:chosen}};
const toast=text=>events.push({event:'toast',text}),markDirty=()=>events.push({event:'dirty'}),flushSave=()=>events.push({event:'flush'}),blurActive=()=>events.push({event:'blur'}),toRoost=()=>events.push({event:'to_roost'}),renderRoost=()=>events.push({event:'render_roost'});
const showOnly=()=>events.push({event:'show_hatch'});const sfx={hatch:()=>events.push({event:'sound',name:'hatch'})};const showMilestone=()=>events.push({event:'show_milestone',id:msQueue[0].id});Date.now=()=>1234;
"#,
    );
    for (start, end) in [
        ("const KINDS = {", "\n};"),
        ("const MILESTONES = [", "\n];"),
    ] {
        script.push_str(block(start, end, true));
    }
    for start in [
        "const XP_K =",
        "const levelOf =",
        "const EGG_KINDS =",
        "const NAMES =",
        "const cleanName =",
        "const rand =",
        "const pick =",
        "const teamDocs =",
        "const teamHatched =",
        "const msReached =",
        "const allWild =",
        "const teamSlots =",
        "function rgb(",
        "function shadeHex(",
        "function mulberry(",
    ] {
        script.push_str(block(start, "\n", true));
    }
    for start in [
        "function confetti(",
        "function hatch(",
        "function finishHatch(",
        "function setFree(",
        "function checkMilestones(",
        "function closeMilestone(",
    ] {
        script.push_str(block(start, "\n}", true));
    }
    script.push_str(r#"const originalConfetti=confetti;confetti=n=>{const start=draws.length;originalConfetti(n);events.push({event:'confetti',count:n,draws:draws.slice(start)});};"#);
    script.push_str(&format!(r#"const cases={};print(JSON.stringify(cases.map(c=>{{me=c.me;others=c.others;STATIC=c.static;reduceMotion=c.reduce;msQueue=[];events=[];draws=[];const rng=mulberry(c.seed);Math.random=()=>{{const x=rng();draws.push(x);return x;}};hatch();const baby=JSON.parse(JSON.stringify(pendingBaby)),suggested=$('babyName').value,beginEvents=events.slice();events=[];
$('babyName').value=c.name;$('hatchBtn').disabled=false;chosen=c.releaseBaby?pendingBaby.id:'old';finishHatch();const finishEvents=events.slice(),finished=JSON.parse(JSON.stringify(me));events=[];
checkMilestones(true);const checkEvents=events.slice(),queue=msQueue.map(m=>m.id);events=[];checkMilestones(false);const repeatEvents=events.slice();
return {{baby,suggested,beginEvents,finishEvents,finished,checkEvents,queue,repeatEvents,me,drawCount:draws.length}};
}})));"#,serde_json::to_string(&cases()).unwrap()));
    let js: Vec<Value> = serde_json::from_slice(&run_js(&script)).unwrap();
    for (c, j) in cases().iter().zip(js) {
        let mut me = c["me"].clone();
        let others = c["others"].as_array().unwrap();
        let static_mode = c["static"].as_bool().unwrap();
        let mut rng = Mulberry32 {
            state: c["seed"].as_u64().unwrap() as u32,
        };
        let mut draws = 0;
        let mut r = Roost::default();
        let events = r
            .begin_hatch(
                &me,
                others,
                static_mode,
                1234,
                c["reduce"].as_bool().unwrap(),
                || {
                    draws += 1;
                    rng.next_unit()
                },
            )
            .unwrap();
        compare(r.pending_baby.as_ref().unwrap(), &j["baby"]);
        assert_eq!(r.suggested_name, j["suggested"].as_str().unwrap());
        compare(&json!(events), &j["beginEvents"]);
        assert_eq!(draws, j["drawCount"].as_u64().unwrap());
        let release = if c["releaseBaby"] == true {
            r.pending_baby.as_ref().unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        } else {
            "old".into()
        };
        let events = r
            .finish_hatch(
                &mut me,
                c["name"].as_str().unwrap(),
                Some(&release),
                2,
                1234,
            )
            .unwrap();
        compare(&me, &j["finished"]);
        compare(&json!(events), &j["finishEvents"]);
        let events = r
            .check_milestones(&mut me, others, static_mode, true, true)
            .unwrap();
        compare(&json!(events), &j["checkEvents"]);
        compare(&json!(r.milestone_queue), &j["queue"]);
        let events = r
            .check_milestones(&mut me, others, static_mode, false, true)
            .unwrap();
        compare(&json!(events), &j["repeatEvents"]);
        compare(&me, &j["me"]);
    }
}
fn run_js(script: &str) -> Vec<u8> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let wire: String = serde_json::to_string(script)
        .unwrap()
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_string()
            } else {
                c.encode_utf16(&mut [0; 2])
                    .iter()
                    .map(|u| format!(r"\u{u:04x}"))
                    .collect()
            }
        })
        .collect();
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
        .write_all(format!("{wire}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn completion_cases() -> Vec<Value> {
    let mut cases = Vec::new();
    for mission in 1..=24 {
        for mode in 0..4 {
            for day in 0..5 {
                let id = format!("m{mission}");
                let mut raw = json!({"xpv":2,"stars":2,"skills":{},"earned":10,"eggs":1,"replays":2,"hatched":5,"built":["bell"],"wild":[{"name":"Wild","kind":"frost"}],"cleared":["m1","m2","m3","m5","m6","m7","m8","m9","m10","m11"],"loadout":["naina","thuri"],"daily":{"day":day,"done":mission%5==0}});
                raw["cleared"].as_array_mut().unwrap().retain(|v| v != &id);
                if mode == 1 {
                    raw["cleared"].as_array_mut().unwrap().push(json!(id));
                }
                raw["crowns"] = json!({});
                raw["crowns"][&id] = json!(mission % 8);
                let input = roost::Completion {
                    mission_id: id,
                    champion: mode == 2,
                    ok: mode != 3,
                    carry: 15.0,
                    banked: 30.0,
                    bonks: mission % 4,
                    elapsed: if mission % 2 == 0 { 20.0 } else { 30.0 },
                    par: 25.0,
                    had_objective: mission % 2 == 0,
                    day,
                    now_ms: 1234,
                };
                cases.push(json!({"me":normalized(raw),"others":[normalized(json!({"built":["bell","barns"],"wild":[{"name":"Fern","kind":"flame"}]}))],"static":mission%2==0,"input":input}));
            }
        }
    }
    cases
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_end_mission_rewards_match() {
    let mut script = String::from(
        r#"let me,others,STATIC,mission,mStats,current;const ri=2,R={dragon:'Dragon'},P={x:0,y:0},S=40,texts=[];const $=()=>({});const releaseControls=()=>{},plog=()=>{},markDirty=()=>{},renderSpells=()=>{};const sfx=new Proxy({},{get:()=>()=>{}});const plural=(n,one,many)=>`${n} ${n===1?one:many}`;const clock=String;const dayKey=()=>current.day;Date.now=()=>current.now_ms;
"#,
    );
    for (start, end) in [
        ("const MISSIONS = [", "\n];"),
        ("const REALMS = [", "\n];"),
        ("const WORDS = {", "\n};"),
        ("const RANKS = [", "\n];"),
        ("const PROJECTS = [", "\n];"),
        ("const MILESTONES = [", "\n];"),
        ("const DAILY = [", "\n];"),
        ("const CROWNS = [", "\n];"),
        ("const CHAMP_LOOKS = [", "\n];"),
    ] {
        script.push_str(block(start, end, true));
    }
    for start in [
        "const DAILY_GEMS =",
        "const BRAVE_BONKS =",
        "const teamDocs =",
        "const teamHatched =",
        "const msReached =",
        "const allWild =",
        "const teamSlots =",
        "const knownWords =",
        "const rankOf =",
        "const spellSlots =",
        "const crownsOf =",
        "const bits3 =",
        "const crownCount =",
        "const teamBuilt =",
        "const thankGift =",
        "function dailyNow(",
    ] {
        script.push_str(block(start, "\n", true));
    }
    for start in [
        "function activeWords(",
        "function equipWord(",
        "function checkDaily(",
        "function champResults(",
        "function bankGems(",
    ] {
        script.push_str(block(start, "\n}", true));
    }
    // Use the actual endMission persistent-reward block; presentation after this
    // boundary only builds results and issues the final save intents.
    script.push_str(block("function endMission(", "  const grew =", false));
    script.push_str("return {me,first,egg,carry:M.carry,banked:M.banked};}\n");
    script.push_str(&format!(r#"const cases={};print(JSON.stringify(cases.map(c=>{{me=c.me;others=c.others;STATIC=c.static;current=c.input;const base=MISSIONS.find(m=>m.id===current.mission_id),def=current.champion?{{...base,id:base.id+'c',base,champ:true,par:current.par,egg:false}}:base;mission={{def,guest:true,carry:current.carry,banked:current.banked,t:current.elapsed,par:current.par,hadOb:current.had_objective}};mStats={{bonks:current.bonks,kills:0}};return endMission(current.ok);}})));"#,serde_json::to_string(&completion_cases()).unwrap()));
    let js: Vec<Value> = serde_json::from_slice(&run_js(&script)).unwrap();
    for (c, j) in completion_cases().iter().zip(js) {
        let mut me = c["me"].clone();
        let i = &c["input"];
        let input = roost::Completion {
            mission_id: i["mission_id"].as_str().unwrap().into(),
            champion: i["champion"].as_bool().unwrap(),
            ok: i["ok"].as_bool().unwrap(),
            carry: i["carry"].as_f64().unwrap(),
            banked: i["banked"].as_f64().unwrap(),
            bonks: i["bonks"].as_u64().unwrap() as u32,
            elapsed: i["elapsed"].as_f64().unwrap(),
            par: i["par"].as_f64().unwrap(),
            had_objective: i["had_objective"].as_bool().unwrap(),
            day: i["day"].as_i64().unwrap(),
            now_ms: 1234,
        };
        let out = roost::complete_mission(
            &mut me,
            c["others"].as_array().unwrap(),
            c["static"].as_bool().unwrap(),
            &input,
        )
        .unwrap();
        compare(&me, &j["me"]);
        assert_eq!(out.first, j["first"]);
        assert_eq!(out.egg, j["egg"]);
        assert_eq!(out.carry, j["carry"].as_f64().unwrap());
        assert_eq!(out.banked, j["banked"].as_f64().unwrap());
    }
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_milestone_close_order_matches() {
    let me = normalized(json!({"seenMs":["song"]}));
    let script = format!(
        r#"{}let me={},msQueue=MILESTONES.slice(),events=[],state='roost';
const markDirty=()=>events.push({{event:'dirty'}}),showMilestone=()=>events.push({{event:'show_milestone',id:msQueue[0].id}}),blurActive=()=>events.push({{event:'blur'}}),renderRoost=()=>events.push({{event:'render_roost'}});
const $=()=>({{set hidden(v){{if(v)events.push({{event:'hide_milestone'}});}}}});
{}const result=[];for(let i=0;i<8;i++){{events=[];closeMilestone();result.push({{events,seen:me.seenMs.slice(),queue:msQueue.map(m=>m.id)}});}}print(JSON.stringify(result));"#,
        block("const MILESTONES = [", "\n];", true),
        serde_json::to_string(&me).unwrap(),
        block("function closeMilestone(", "\n}", true)
    );
    let result: Vec<Value> = serde_json::from_slice(&run_js(&script)).unwrap();
    let mut me = me;
    let mut r = Roost {
        milestone_queue: [
            "first", "song", "gifts", "wing", "golden", "festival", "skies",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        ..Default::default()
    };
    for j in result {
        let e = r.close_milestone(&mut me, true).unwrap();
        compare(&json!(e), &j["events"]);
        compare(&me["seenMs"], &j["seen"]);
        compare(&json!(r.milestone_queue), &j["queue"]);
    }
}
