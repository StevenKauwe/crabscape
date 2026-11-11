use dragon_game::profile::{self, ProfileError};
use serde_json::{Value, json};

fn fixtures() -> Vec<Value> {
    let mut cases = vec![
        json!({"build":{"name":"José 龙","dname":"Naína 𐐀"}}),
        json!({"build":{"name":"José 龙","dname":"Naína 𐐀"},"rid":"龙𐐀","last":{"text":"你好！ Café."},"party":[{"id":"x龙","name":"𐐀①龍","kind":"frost"}]}),
        Value::Null,
        json!(false),
        json!("wrong"),
        json!([]),
        json!({}),
        json!({"xpv":2}),
        json!({"build":{"name":"Nova!","dname":"Ember!","path":"forge","crown":true,"breath":99,"horn":"#ffcf3f","cloak":"#d9a520"},"party":[{"bonded":true,"xp":175},{"id":" a-1! ","name":"Pip!","kind":"star","color":"#AbC123","xp":"45"}],"main":"a1","cleared":["m4","m4","m8","m99"],"built":["bell","bell","star"],"briefed":["m1","m1"],"loadout":["naina","naina","faro","sila","mira"],"wild":[{"name":" Fern!","kind":"bond"}],"last":{"text":"Hello! <ok>","at":"12"}}),
        json!({"xpv":"2","party":[{"bonded":[],"xp":"60"}],"skills":[],"stars":"99.9","seenIntro":null,"daily":{"day":null,"done":[]},"craft":{"tea":"0x2","tonic":[3],"saddles":{},"armor":2.9,"bellows":true},"eggs":99,"charms":{"heart":100,"pouch":"2.8"},"crowns":{"m1":8,"m24":"3.7","m99":7},"seenMs":["first","first","no"],"seenGrow":["ride","ride"],"kindsSeen":["star","flame","flame"]}),
    ];
    for xpv in [Value::Null, json!(2), json!("2")] {
        for xp in [0.0, 14.0, 15.0, 60.0, 1000.0, 1e9] {
            cases.push(json!({"xpv":xpv,"party":[{"bonded":true,"xp":xp}],"cleared":["m4","m12","m24"],"skills":null}));
        }
    }
    let mut many = json!({"party":[],"wild":[],"rid":"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ","daily":{"day":"-2.5","done":"false"}});
    for i in 0..45 {
        many["party"].as_array_mut().unwrap().push(json!({"kind":"flame","name":"Dragon","id":if i % 2 == 0 {json!("")}else{json!("!!!")},"xp":i}));
        many["wild"]
            .as_array_mut()
            .unwrap()
            .push(json!({"kind":"frost","name":format!("Wild {i}"),"realm":"sea"}));
    }
    cases.push(many);
    cases
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
fn defaults_progression_and_unsupported_inputs_are_explicit() {
    let default =
        profile::normalize_profile(&Value::Null, "rider3", 2, || panic!("no RNG")).unwrap();
    assert_eq!(default, profile::new_profile("rider3"));
    assert!(default.get("rid").is_none());
    let p = profile::normalize_profile(
        &json!({"cleared":["m4","m4"],"party":[{"bonded":true,"xp":60}]}),
        "rider3",
        2,
        || panic!("no RNG"),
    )
    .unwrap();
    assert_eq!(p["stars"], json!(3));
    assert_eq!(p["seenIntro"], true);
    assert_eq!(p["party"][0]["xp"], 160.0);
    assert_eq!(profile::clean_name(&json!("Émber")), Ok("Émber".into()));
    assert_eq!(
        profile::normalize_profile(&json!({"loadout":["toString"]}), "rider3", 2, String::new),
        Err(ProfileError::PrototypeKey)
    );
    assert_eq!(
        profile::clean_name(&json!("aaaaaaaaaaaaa𐐀")),
        Err(ProfileError::UnsupportedString)
    );
    assert_eq!(
        profile::clean_build(&json!({"name":"", "dname":{}})).unwrap(),
        Value::Null
    );
    let raw = fixtures().pop().unwrap();
    let original = raw.clone();
    let mut draws = 0;
    let p = profile::normalize_profile(&raw, "rider3", 2, || {
        draws += 1;
        "di".into()
    })
    .unwrap();
    assert_eq!(draws, 23, "even discarded extras consume fallback IDs");
    assert_eq!(p["party"].as_array().unwrap().len(), 5);
    assert_eq!(p["wild"].as_array().unwrap().len(), 40);
    assert_eq!(raw, original, "source is untouched");
    for case in fixtures() {
        profile::normalize_profile(&case, "rider3", 2, || "di".into()).unwrap();
    }
}
#[test]
fn javascript_number_and_truthiness_are_not_rust_parsing() {
    for (v, n) in [
        (json!(null), 0.0),
        (json!(true), 1.0),
        (json!([]), 0.0),
        (json!([null]), 0.0),
        (json!([["0x20"]]), 32.0),
        (json!("\u{feff}3\u{00a0}"), 3.0),
        (json!("0b11"), 3.0),
        (json!("0o10"), 8.0),
        (json!("Infinity"), 0.0),
        (json!("inf"), 0.0),
    ] {
        assert_eq!(profile::num(Some(&v), 1e9).unwrap(), n);
    }
    assert!(profile::truthy(&json!([])));
    assert!(profile::truthy(&json!({})));
    assert!(profile::num(Some(&json!({"toString":null})), 1e9).is_err());
}
fn block(start: &str, end: &str, include_end: bool) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(start).unwrap()..];
    &tail[..tail.find(end).unwrap() + if include_end { end.len() } else { 0 }]
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_source_profile_normalization_matches_synthetic_saves() {
    let mut script = String::new();
    for (start, end) in [
        ("const RIDERS = [", "\n];"),
        ("const PATHS = {", "\n};"),
        ("const LOOK = {", "\n};"),
        ("const KINDS = {", "\n};"),
        ("const WORDS = {", "\n};"),
        ("const REALMS = [", "\n];"),
        ("const MISSIONS = [", "\n];"),
        ("const MILESTONES = [", "\n];"),
        ("const BRIEFS = {", "\n};"),
        ("const CHARMS = [", "\n];"),
        ("const GROW_SCENES = [", "\n];"),
        ("const PROJECTS = [", "\n];"),
        ("const CHAMP_LOOKS = [", "\n];"),
    ] {
        script.push_str(block(start, end, true));
        script.push('\n');
    }
    script.push_str(block("const START_KINDS =", "\n", true));
    script.push_str(block("const EGG_KINDS =", "\n", true));
    script.push_str(block("const clamp =", "// Fill a save slot", false));
    script.push_str(block("const XP_K =", "function stageOf", false));
    script.push_str(block(
        "function newProfile(",
        "async function loadRider(",
        false,
    ));
    script.push_str(&format!("\nMath.random=()=>0.5;const cases={};print(JSON.stringify(cases.map(c=>normalizeProfile(c,2))));",serde_json::to_string(&fixtures()).unwrap()));
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
    // jsc readline interprets stdin bytes as Latin-1. Escape the JSON envelope
    // to ASCII so Unicode fixture text reaches the JS parser unchanged.
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", wire).as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let js: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    for (raw, js) in fixtures().iter().zip(js) {
        let rust = profile::normalize_profile(raw, "rider3", 2, || "di".into()).unwrap();
        compare(&rust, &js);
    }
}
