use dragon_game::{
    profile,
    random::Mulberry32,
    roost_gate::{self, Event, Gate},
};
use serde_json::{Value, json};
fn profile() -> Value {
    profile::normalize_profile(&json!({"eggs":1,"eggAwake":true}), "rider3", 2, String::new)
        .unwrap()
}
fn snapshot(g: &Gate, me: &Value, draws: usize) -> Value {
    json!({"state":g.state,"question":g.question,"done":g.done,"run":g.run,"seen":g.seen,"input":g.input,"locked":g.locked,"card_locked":g.card_locked,"verdict":g.verdict,"verdict_class":g.verdict_class,"frozen":g.frozen,"thaw_hidden":g.thaw_hidden,"freeze_hidden":g.freeze_hidden,"freeze_title":g.freeze_title,"freeze_note":g.freeze_note,"freeze_clock":g.freeze_clock,"eggAwake":me["eggAwake"],"draws":draws})
}
fn cases() -> Vec<Value> {
    let mut correct = vec![
        json!({"op":"begin","now":0}),
        json!({"op":"key","key":"1","now":1}),
        json!({"op":"advance","now":999}),
        json!({"op":"advance","now":1000}),
        json!({"op":"answer","now":1000}),
    ];
    let mut now = 1000;
    for i in 0..5 {
        correct.push(json!({"op":"correct","now":now}));
        correct.push(json!({"op":"answer","now":now+1}));
        let delay = if i == 4 { 900 } else { 600 };
        correct.push(json!({"op":"advance","now":now+delay-1}));
        now += delay;
        correct.push(json!({"op":"advance","now":now}));
    }
    let wrong = vec![
        json!({"op":"begin","now":100}),
        json!({"op":"advance","now":1100}),
        json!({"op":"key","key":"999","now":1100}),
        json!({"op":"key","key":"9","now":1100}),
        json!({"op":"answer","now":1100}),
        json!({"op":"advance","now":2399}),
        json!({"op":"advance","now":5000}),
        json!({"op":"finish","now":64999}),
        json!({"op":"update","now":64999}),
        json!({"op":"update","now":65000}),
        json!({"op":"update","now":65001}),
        json!({"op":"finish","now":65001}),
    ];
    let restart = vec![
        json!({"op":"begin","now":0}),
        json!({"op":"begin","now":500}),
        json!({"op":"advance","now":1000}),
        json!({"op":"advance","now":1500}),
        json!({"op":"key","key":"clear","now":1500}),
        json!({"op":"key","key":"0x0","now":1500}),
        json!({"op":"answer","now":1500}),
        json!({"op":"menu","now":1600}),
        json!({"op":"advance","now":2800}),
        json!({"op":"update","now":62800}),
    ];
    let mut out = Vec::new();
    for test in [false, true] {
        for steps in [&correct, &wrong, &restart] {
            out.push(json!({"test":test,"steps":steps,"seed":1234}));
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
        (Value::Number(a), Value::Number(b)) => {
            assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8)
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (k, a) in a {
                compare(a, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn stale_unlocks_wrong_freeze_and_save_intents() {
    let mut g = Gate::new(2, false);
    let mut me = profile();
    let mut rng = Mulberry32 { state: 42 };
    g.begin(&me, 0.0, || rng.next_unit()).unwrap();
    g.begin(&me, 500.0, || rng.next_unit()).unwrap();
    g.advance(&mut me, 1000.0, || rng.next_unit()).unwrap();
    assert!(g.locked);
    g.advance(&mut me, 1500.0, || rng.next_unit()).unwrap();
    assert!(!g.locked);
    g.key("999");
    g.answer(1500.0).unwrap();
    let e = g.advance(&mut me, 10000.0, || rng.next_unit()).unwrap();
    assert_eq!(g.frozen.as_ref().unwrap()["until"], 70000.0);
    assert_eq!(me["eggAwake"], false);
    assert!(e.iter().any(|e| matches!(e, Event::SaveFrozen { .. })));
    assert_eq!(
        roost_gate::load_frozen(g.frozen.as_ref().unwrap(), 70000.0, false).unwrap(),
        None
    );
    assert!(
        roost_gate::load_frozen(g.frozen.as_ref().unwrap(), 69999.0, false)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        roost_gate::load_frozen(g.frozen.as_ref().unwrap(), 69999.0, true).unwrap(),
        None
    );
    g.update_freeze(70000.0).unwrap();
    assert!(!g.thaw_hidden);
    g.finish_freeze(70000.0).unwrap();
    assert_eq!(g.state, "roost");
}
#[test]
fn inverse_questions_are_duplicates_and_input_caps_before_append() {
    let mut g = Gate::default();
    let mut draws = [0.0, 1.0 / 13.0, 1.0 / 13.0, 0.0, 12.0 / 13.0, 12.0 / 13.0].into_iter();
    let first = g.next_question(|| draws.next().unwrap()).unwrap();
    let second = g.next_question(|| draws.next().unwrap()).unwrap();
    assert_eq!(first.n, 0);
    assert_eq!(second.n, 144);
    assert_eq!(g.seen, vec![1, 168]);
    g.state = "riddle".into();
    g.key("12");
    g.key("34");
    assert_eq!(g.input, "1234");
    g.key("5");
    assert_eq!(g.input, "1234");
    g.key("clear");
    assert_eq!(g.input, "");
    assert_eq!(roost_gate::day_key(0.0, 60.0).unwrap(), -1.0);
    assert_eq!(roost_gate::day_key(86400000.0, 0.0).unwrap(), 1.0);
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_times_gate_freeze_snapshots_match() {
    let mut script = String::from(
        r#"let me,TEST_FLIGHT,state,riddle,riddleDone,timesRun,timesSeen,frozen,nodes,timers,now,draws,cardLocked;const ri=2,R={name:'Nova'},TIMES_TO_HATCH=5,FREEZE_SECONDS=60,pressed={clear(){}};
const node=()=>({value:'',disabled:false,hidden:true,textContent:'',className:'verdict',children:[],classList:{add(){cardLocked=true},remove(){cardLocked=false}}});const $=id=>nodes[id]||(nodes[id]=node());
const initAudio=()=>{},releaseControls=()=>{},showOnly=()=>{},markDirty=()=>{},blurActive=()=>{},lsSet=()=>{},speechSynthesis={cancel(){}},sfx=new Proxy({},{get:()=>()=>{}});const toRoost=()=>{state='roost'},hatch=()=>{state='hatch'};Date.now=()=>now;const setTimeout=(fn,delay)=>timers.push({at:now+delay,fn});
"#,
    );
    for start in [
        "const rand =",
        "const cleanText =",
        "function mulberry(",
        "function setRiddleLock(",
        "function showTimesRun(",
    ] {
        script.push_str(block(start, "\n", true));
    }
    for start in [
        "function nextTimes(",
        "function askTimes(",
        "function eggReady(",
        "function timesKey(",
        "function answerTimes(",
        "function freeze(",
        "function updateFreeze(",
        "function finishFreeze(",
    ] {
        script.push_str(block(start, "\n}", true));
    }
    script.push_str(&format!(r#"const cases={};const results=cases.map(c=>{{me={{eggs:1,eggAwake:true}};TEST_FLIGHT=c.test;state='menu';riddle=null;riddleDone=false;timesRun=0;timesSeen=new Set();frozen=null;nodes={{}};timers=[];draws=0;cardLocked=false;const rng=mulberry(c.seed);Math.random=()=>{{draws++;return rng();}};
return c.steps.map(s=>{{now=s.now;if(s.op==='begin')eggReady();else if(s.op==='key')timesKey(s.key);else if(s.op==='correct'){{$('riddleInput').value=String(riddle.n);answerTimes();}}else if(s.op==='answer')answerTimes();else if(s.op==='menu')state='menu';else if(s.op==='update')updateFreeze();else if(s.op==='finish')finishFreeze();else if(s.op==='advance'){{while(true){{timers.sort((a,b)=>a.at-b.at);if(!timers.length||timers[0].at>now)break;timers.shift().fn();}}}}
return {{state,question:riddle,done:riddleDone,run:timesRun,seen:[...timesSeen],input:$('riddleInput').value,locked:$('riddleCheck').disabled,card_locked:cardLocked,verdict:$('riddleVerdict').textContent,verdict_class:$('riddleVerdict').className,frozen,thaw_hidden:$('thawBtn').hidden,freeze_hidden:$('freezeScreen').hidden,freeze_title:$('freezeTitle').textContent,freeze_note:$('freezeNote').textContent,freeze_clock:$('freezeClock').textContent,eggAwake:me.eggAwake,draws}};}});}});print(JSON.stringify(results));"#,serde_json::to_string(&cases()).unwrap()));
    let js: Vec<Vec<Value>> = serde_json::from_slice(&run_js(&script)).unwrap();
    for (c, expected) in cases().iter().zip(js) {
        let mut g = Gate::new(2, c["test"].as_bool().unwrap());
        let mut me = profile();
        let mut draws = 0;
        let mut rng = Mulberry32 { state: 1234 };
        for (s, j) in c["steps"].as_array().unwrap().iter().zip(expected) {
            let now = s["now"].as_f64().unwrap();
            let mut next = || {
                draws += 1;
                rng.next_unit()
            };
            match s["op"].as_str().unwrap() {
                "begin" => {
                    g.begin(&me, now, &mut next).unwrap();
                }
                "key" => {
                    g.key(s["key"].as_str().unwrap());
                }
                "correct" => {
                    g.input = g.question.unwrap().n.to_string();
                    g.answer(now).unwrap();
                }
                "answer" => {
                    g.answer(now).unwrap();
                }
                "menu" => g.state = "menu".into(),
                "update" => {
                    g.update_freeze(now).unwrap();
                }
                "finish" => {
                    g.finish_freeze(now).unwrap();
                }
                "advance" => {
                    g.advance(&mut me, now, &mut next).unwrap();
                }
                _ => panic!("op"),
            };
            compare(&snapshot(&g, &me, draws), &j);
        }
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

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_day_and_frozen_reload_boundaries_match() {
    let mut cases = Vec::new();
    for now in [
        -86400001.0,
        -1.0,
        0.0,
        1.0,
        86399999.0,
        86400000.0,
        1800000000000.0,
    ] {
        for offset in [-720.0, -330.0, 0.0, 420.0, 480.0, 840.0] {
            for test in [false, true] {
                for until in [
                    json!(null),
                    json!(now),
                    json!(now + 0.5),
                    json!((now + 1000.0).to_string()),
                    json!("bad"),
                ] {
                    cases.push(json!({"now":now,"offset":offset,"test":test,"saved":{"until":until,"answer":"2 times 3 is 6","who":2,"unknown":[1,2]}}));
                }
            }
        }
    }
    cases.push(json!({"now":0,"offset":0,"test":false,"saved":null}));
    let script = format!(
        r#"let current;const Date=class{{static now(){{return current.now}}getTimezoneOffset(){{return current.offset}}}};let TEST_FLIGHT;const lsGet=()=>current.saved;{}{}const cases={};print(JSON.stringify(cases.map(c=>{{current=c;TEST_FLIGHT=c.test;return {{day:dayKey(),frozen:loadFrozen()}};}})));"#,
        block("const dayKey =", "\n", true),
        block("function loadFrozen(", "\n", true),
        serde_json::to_string(&cases).unwrap()
    );
    let js: Vec<Value> = serde_json::from_slice(&run_js(&script)).unwrap();
    for (c, j) in cases.iter().zip(js) {
        let now = c["now"].as_f64().unwrap();
        assert_eq!(
            roost_gate::day_key(now, c["offset"].as_f64().unwrap()).unwrap(),
            j["day"].as_f64().unwrap()
        );
        compare(
            &json!(
                roost_gate::load_frozen(&c["saved"], now, c["test"].as_bool().unwrap()).unwrap()
            ),
            &j["frozen"],
        );
    }
}
