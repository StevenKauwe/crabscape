use dragon_game::{
    profile,
    progression::{self, Command, Error},
};
use serde_json::{Value, json};
fn normalized(raw: Value) -> Value {
    profile::normalize_profile(&raw, "rider3", 2, || "di".into()).unwrap()
}
fn command(v: &Value) -> Command {
    let id = v["id"].as_str().unwrap_or("").to_owned();
    match v["op"].as_str().unwrap() {
        "xp" => Command::AddXp {
            dragon: id,
            amount: v["amount"].as_f64().unwrap(),
        },
        "feed" => Command::Feed(id),
        "treat" => Command::Treat(id),
        "craft" => Command::Craft(id),
        "skill" => Command::LearnSkill(id),
        "main" => Command::SelectMain(id),
        "swap" => Command::Swap,
        "spend" => Command::Spend {
            cost: v["amount"].as_f64().unwrap(),
            text: "fixture spend".into(),
        },
        _ => panic!("fixture op"),
    }
}
fn cases() -> Vec<Value> {
    let mut out = Vec::new();
    for static_mode in [false, true] {
        for path in ["shadow", "forge", "storm", "guardian"] {
            let me = normalized(
                json!({"xpv":2,"earned":2000,"hatched":8,"party":[{"bonded":true,"xp":0},{"id":"extra","name":"Pip","kind":"flame","xp":0}],"build":{"name":"Nova","dname":"Dragon","path":path},"stars":30,"skills":{},"cleared":["m2","m3"],"craft":{"tea":1,"tonic":1}}),
            );
            let other = normalized(
                json!({"earned":1000,"spent":50,"hatched":8,"craft":{"treatsMade":2,"tea":1,"tonic":2,"saddles":true}}),
            );
            let mut commands = vec![
                json!({"op":"feed","id":"bond"}),
                json!({"op":"treat","id":"extra"}),
                json!({"op":"xp","id":"bond","amount":2000}),
                json!({"op":"swap"}),
                json!({"op":"main","id":"bond"}),
                json!({"op":"spend","amount":5.5}),
            ];
            for id in [
                "tea",
                "tea",
                "tea",
                "tonic",
                "saddles",
                "saddles",
                "armor",
                "armor",
                "armor",
                "armor",
                "bellows",
                "treatsMade",
            ] {
                commands.push(json!({"op":"craft","id":id}));
            }
            for id in ["breath", "rider", "magic"] {
                for _ in 0..6 {
                    commands.push(json!({"op":"skill","id":id}));
                }
            }
            commands.push(json!({"op":"xp","id":"bond","amount":20000}));
            out.push(json!({"me":me,"others":[other],"static":static_mode,"commands":commands}));
        }
    }
    for stars in [0, 4, 10] {
        for clear in [json!([]), json!(["m2", "m3"])] {
            out.push(json!({"me":normalized(json!({"stars":stars,"skills":{"magic":4},"cleared":clear})),"others":[],"static":true,"commands":[{"op":"skill","id":"magic"},{"op":"feed","id":"bond"},{"op":"treat","id":"bond"},{"op":"craft","id":"tea"},{"op":"swap"}]}));
        }
    }
    out
}
#[test]
fn errors_do_not_mutate_and_peers_remain_read_only() {
    let mut me = normalized(json!({"earned":9,"skills":{"magic":4},"stars":9}));
    let original = me.clone();
    assert_eq!(
        progression::apply(&mut me, &[], true, Command::Feed("bond".into()), 1.0),
        Err(Error::NotEnoughGems)
    );
    assert_eq!(
        progression::apply(&mut me, &[], true, Command::LearnSkill("magic".into()), 1.0),
        Err(Error::NeedsWords)
    );
    assert_eq!(me, original);
    let peers = vec![normalized(
        json!({"earned":100,"craft":{"tonic":3,"saddles":true},"hatched":16}),
    )];
    let before = peers.clone();
    progression::apply(&mut me, &peers, false, Command::Feed("bond".into()), 42.0).unwrap();
    assert_eq!(me["party"][0]["xp"], 43.75);
    assert_eq!(me["spent"], 10.0);
    assert_eq!(peers, before);
    let e = progression::apply(
        &mut me,
        &peers,
        false,
        Command::AddXp {
            dragon: "bond".into(),
            amount: 20000.0,
        },
        43.0,
    )
    .unwrap();
    assert_eq!(me["party"][0]["xp"], 14440.0);
    assert!(
        matches!(&e[0],progression::Event::Growth { level_before:2,level_after:20,stage_before:0,stage_after:3,stars,riding_unlocked:true,.. } if *stars==18.0)
    );
    let before = me.clone();
    assert_eq!(
        progression::apply(
            &mut me,
            &peers,
            false,
            Command::AddXp {
                dragon: "bond".into(),
                amount: f64::NAN
            },
            43.0
        ),
        Err(Error::InvalidNumber)
    );
    assert_eq!(me, before);
}
fn block(start: &str, end: &str, inclusive: bool) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(start).unwrap()..];
    &tail[..tail.find(end).unwrap() + if inclusive { end.len() } else { 0 }]
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
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn source_commands_and_aggregations_match() {
    let mut script = String::from(
        r#"let me,others,STATIC,R;const state='roost',P={},S=40,wingState=new Map();const RIDE_LEVEL=8;
const markDirty=()=>{},renderParty=()=>{},toast=()=>{},flushSave=()=>{},renderRoost=()=>{},updateHud=()=>{},showGrowScene=()=>{},showStory=()=>{},toSkills=()=>{},slugOf=x=>x;
const SKILL_STORIES={};const sfx=new Proxy({}, {get:()=>()=>{}});const plural=(n,one,many)=>`${n} ${n===1?one:many}`;Date.now=()=>1234;
"#,
    );
    for (start, end) in [
        ("const STAGES = [", "\n];"),
        ("const CRAFTS = {", "\n};"),
        ("const MILESTONES = [", "\n];"),
        ("const WORDS = {", "\n};"),
        ("const SKILLS = {", "\n};"),
        ("const RIDER_SKILLS = {", "\n};"),
    ] {
        script.push_str(block(start, end, true));
        script.push('\n');
    }
    script.push_str(block("const XP_K =", "// Daddio's own dragon", false));
    script.push_str(block("const bonded =", "const specialCdMax =", false));
    for start in [
        "const teamHatched =",
        "const msReached =",
        "const xpMult =",
        "const knownWords =",
        "const riding =",
    ] {
        script.push_str(block(start, "\n", true));
    }
    for start in [
        "function addXP(",
        "function spend(",
        "function roostAction(",
        "function skillBlock(",
        "function learnSkill(",
        "function doCraft(",
    ] {
        script.push_str(block(start, "\n}", true));
        script.push('\n');
    }
    script.push_str(block("function branchSteps(", "\n", true));
    script.push_str(&format!(r#"
const cases={};const result=cases.map(c=>{{me=c.me;others=c.others;STATIC=c.static;R={{path:me.build?.path||'shadow',name:'Nova'}};const snapshots=[];
const summary=()=>({{craft:teamCraft(),hoard:hoard(),treats_left:treatsLeft(),hatched:teamHatched(),xp_multiplier:xpMult()}});
const initial=summary();for(const cmd of c.commands){{const d=me.party.find(d=>d.id===cmd.id);
if(cmd.op==='xp')addXP(d,cmd.amount);else if(cmd.op==='feed'||cmd.op==='treat'||cmd.op==='main')roostAction(cmd.op,cmd.id,{{}});
else if(cmd.op==='craft')doCraft(['treatsMade','tea','tonic'].includes(cmd.id)?'herbs':'forge',cmd.id);
else if(cmd.op==='skill')learnSkill(cmd.id);else if(cmd.op==='swap')swapDragon();else if(cmd.op==='spend')spend(cmd.amount,'fixture spend');
snapshots.push(JSON.parse(JSON.stringify(me)));}}return {{initial,snapshots,final:summary()}};}});print(JSON.stringify(result));
"#,serde_json::to_string(&cases()).unwrap()));
    use std::io::Write;
    use std::process::{Command as Process, Stdio};
    let wire: String = serde_json::to_string(&script)
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
        Process::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
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
    let js: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    for (case, js) in cases().iter().zip(js) {
        let mut me = case["me"].clone();
        let others = case["others"].as_array().unwrap();
        let static_mode = case["static"].as_bool().unwrap();
        compare(
            &serde_json::to_value(progression::team_summary(&me, others, static_mode)).unwrap(),
            &js["initial"],
        );
        for (cmd, expected) in case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .zip(js["snapshots"].as_array().unwrap())
        {
            let _ = progression::apply(&mut me, others, static_mode, command(cmd), 1234.0);
            compare(&me, expected);
        }
        compare(
            &serde_json::to_value(progression::team_summary(&me, others, static_mode)).unwrap(),
            &js["final"],
        );
    }
}
