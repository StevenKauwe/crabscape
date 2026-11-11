//! Optional Mac source oracle, following dragon-game's existing JavaScriptCore tests.
use super::*;
use serde_json::{Value, json};
fn source_between(start: &str, end: &str) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let source = &source[source.find(start).unwrap()..];
    &source[..source.find(end).unwrap()]
}
fn row(n: &Note) -> Value {
    json!([
        n.frequency,
        n.duration,
        match n.wave {
            Wave::Sine => "sine",
            Wave::Triangle => "triangle",
            Wave::Square => "square",
            Wave::Sawtooth => "sawtooth",
            Wave::Lowpass => "lowpass",
            Wave::Highpass => "highpass",
            Wave::Bandpass => "bandpass",
        },
        n.volume,
        n.end_frequency,
        n.delay,
        n.music
    ])
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{a} vs {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
fn run(script: &str) -> Value {
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
    let encoded: String = serde_json::to_string(script)
        .unwrap()
        .encode_utf16()
        .map(|c| {
            if c < 128 {
                char::from_u32(u32::from(c)).unwrap().to_string()
            } else {
                format!(r"\u{c:04x}")
            }
        })
        .collect();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{encoded}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn original_javascript_effect_recipes_match() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    let mut synth = Synth::default();
    for name in [
        "gold",
        "thud",
        "poof",
        "grr",
        "throw",
        "zap",
        "bonk",
        "hatch",
        "grow",
        "wave",
        "win",
        "pick",
        "spell",
        "shadow",
        "dadBoom",
        "megaLaunch",
        "megaBoom",
        "rimshot",
        "smash",
        "wiggle",
        "correct",
        "wrong",
        "freeze",
        "thaw",
    ] {
        cases.push(json!([name, null, null]));
        expected.push(json!(
            synth
                .recipe(named_effect(name).unwrap())
                .iter()
                .map(row)
                .collect::<Vec<_>>()
        ));
    }
    for i in 0..15 {
        cases.push(json!(["gem", i, null]));
        expected.push(json!(
            synth
                .recipe(Effect::Gem(i))
                .iter()
                .map(row)
                .collect::<Vec<_>>()
        ));
    }
    for (name, breath) in [
        ("spark", Breath::Spark),
        ("spark", Breath::Spark),
        ("spark", Breath::Spark),
        ("boulder", Breath::Boulder),
        ("frost", Breath::Frost),
        ("star", Breath::Star),
        ("fire", Breath::Fire),
    ] {
        cases.push(json!(["breath", name, null]));
        expected.push(json!(
            synth
                .recipe(Effect::Breath(breath))
                .iter()
                .map(row)
                .collect::<Vec<_>>()
        ));
    }
    for (name, path) in [
        ("shadow", Path::Shadow),
        ("storm", Path::Storm),
        ("guardian", Path::Guardian),
        ("other", Path::Other),
    ] {
        cases.push(json!(["weapon", null, name]));
        expected.push(json!(
            synth
                .recipe(Effect::Weapon(path))
                .iter()
                .map(row)
                .collect::<Vec<_>>()
        ));
    }
    let script = format!(
        r#"let rows=[], sparkTick=0,R={{path:'other'}}; const SCALE=[0,2,4,7,9,12,14,16,19,21,24]; function tone(f,d,w='sine',v=0.2,f2=null,delay=0){{rows.push([f,d,w,v,f2,delay,false]);}} function noise(d,v,f,w='lowpass',delay=0){{tone(f,d,w,v,null,delay);}} {} const cases={};print(JSON.stringify(cases.map(c=>{{rows=[];R.path=c[2];sfx[c[0]](c[1]);return rows;}})));"#,
        source_between("const sfx = {", "/* ---------- Sizing"),
        json!(cases)
    );
    compare(&json!(expected), &run(&script));
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn original_javascript_music_scheduler_matches() {
    let mut expected = Vec::new();
    for realm in 0..7 {
        for boss in [false, true] {
            let mut m = Music::default();
            let scene = if realm == 6 {
                MusicScene::Calm
            } else {
                MusicScene::Realm(realm)
            };
            let mut frames = Vec::new();
            for frame in 0..100 {
                frames.push(json!(
                    m.tick(frame as f64 * 0.09, scene, boss, || 0.5)
                        .iter()
                        .map(row)
                        .collect::<Vec<_>>()
                ));
            }
            expected.push(json!(frames));
        }
    }
    let definitions = source_between("const MUSIC = {", "let musicOn =");
    let tick = source_between(
        "function musicTick()",
        "setInterval(() => { try { musicTick();",
    );
    let script = format!(
        r#"{definitions} const AC={{state:'running',currentTime:0,createGain:()=>({{gain:{{}},connect:()=>{{}}}})}};const master={{}},noiseBuf={{}},music={{key:null,beat:0,next:0,deg:2}};let musicGain=null,rows=[],want='calm',state='play',enemies=[];const mtof=n=>440*2**((n-69)/12),clamp=(x,a,b)=>Math.max(a,Math.min(b,x)),pick=a=>a[Math.floor(a.length*0.5)];Math.random=()=>0.5;function musicWant(){{return want;}}function mnote(f,t,d,w,v){{rows.push([f,d,w,v,null,Math.max(0,t-AC.currentTime),true]);}}function mdrum(t,f,d,v){{rows.push([f,d,f<500?'lowpass':'bandpass',v,null,Math.max(0,t-AC.currentTime),true]);}} {tick} const results=[];for(const key of ['hollow','wood','frost','sea','ember','stars','calm'])for(const boss of [false,true]){{music.key=null;music.beat=0;music.next=0;music.deg=2;want=key;enemies=boss?[{{boss:true}}]:[];let frames=[];for(let i=0;i<100;i++){{AC.currentTime=i*0.09;rows=[];musicTick();frames.push(rows);}}results.push(frames);}}print(JSON.stringify(results));"#
    );
    compare(&json!(expected), &run(&script));
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn original_javascript_narration_pieces_match() {
    let texts = [
        "Hello! \"Come here.\" Then we fly.",
        "Wait... What?! \"Yes! Fly now.\"",
        "Clouds float. Stars sing… We are home!",
        "\"Hello\" said Dad.",
    ];
    let script = format!(
        "{} print(JSON.stringify({}.map(t=>narrPieces(t))));",
        source_between("function narrPieces(text)", "const wordsOf"),
        json!(texts)
    );
    let expected: Vec<Value> = texts
        .iter()
        .map(|t| {
            json!(
                narration_pieces(t)
                    .iter()
                    .map(|p| json!({"text":p.text,"q":p.quoted}))
                    .collect::<Vec<_>>()
            )
        })
        .collect();
    compare(&json!(expected), &run(&script));
}
