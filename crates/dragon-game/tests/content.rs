use dragon_game::content::*;
use serde_json::json;
#[test]
fn journal_and_shared_unlocks_preserve_source_order() {
    let p = json!({"cleared":["m12","m4","m1"]});
    assert_eq!(journal(&p), ["intro", "m1", "m4", "m12", "part2"]);
    assert!(mission_unlocked(&p, &[], "m5"));
    assert!(mission_unlocked(&p, &[], "m1c"));
    assert!(!mission_unlocked(&p, &[], "m5c"));
    assert!(mission_unlocked(&p, &[json!({"cleared":["m8"]})], "m5c"));
    assert!(!mission_unlocked(&p, &[], "m25"));
    assert_eq!(catalog()["story"].as_object().unwrap().len(), 26);
    assert_eq!(catalog()["briefs"].as_object().unwrap().len(), 24);
    assert!(briefing("m24").unwrap().contains("Star Swallower"));
}
#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires system JavaScriptCore"]
fn exact_original_content_and_unlock_rules() {
    use serde_json::Value;
    let source = include_str!("../../../web/dragon/index.html");
    let chunk = &source[source.find("const STORY = {").unwrap()
        ..source.find("// A story page is in the journal").unwrap()];
    let realms = &source
        [source.find("const REALMS = [").unwrap()..source.find("// Each wave lists").unwrap()];
    let start = source.find("const MISSIONS = [").unwrap();
    let missions = &source[start..start + source[start..].find("];\n").unwrap() + 2];
    let unlock_start = source.find("function missionUnlocked(").unwrap();
    let unlock = &source
        [unlock_start..unlock_start + source[unlock_start..].find("/* ---------- Sound").unwrap()];
    let mut profiles = vec![];
    let mut expected = vec![];
    for bits in 0..64 {
        let clears = (0..6)
            .filter(|i| bits & (1 << i) != 0)
            .map(|i| format!("m{}", (i + 1) * 4))
            .collect::<Vec<_>>();
        let p = json!({"cleared":clears});
        profiles.push(p.clone());
        expected.push(json!({"quest":quest_line(&p),"journal":journal(&p),"unlocks":(1..=24).flat_map(|i|[format!("m{i}"),format!("m{i}c")]).map(|id|mission_unlocked(&p,&[json!({"cleared":["m2","m8"]})],&id)).collect::<Vec<_>>()}));
    }
    let script = format!(
        r#"{missions}{realms}{chunk}
let me,others=[{{cleared:['m2','m8']}}]; const bossOf=realm=>MISSIONS.find(m=>m.realm===realm&&m.boss); {unlock}
const profiles={profiles};const outcomes=profiles.map(p=>{{me=p;return {{quest:questLine(p),journal:STORY_ORDER.filter(id=>id==='intro'||(id==='part2'?p.cleared.includes('m12'):p.cleared.includes(id))),unlocks:MISSIONS.flatMap(m=>[missionUnlocked(m),missionUnlocked({{...m,champ:true}})])}};}});
print(JSON.stringify({{catalog:{{story:STORY,quests:QUESTS,briefs:BRIEFS,no_wick_briefs:NO_WICK_BRIEFS,charms:CHARMS,ranks:RANKS,grow_scenes:GROW_SCENES,skill_stories:SKILL_STORIES,projects:PROJECTS,story_order:STORY_ORDER,realms:REALMS}},outcomes}}));"#,
        profiles = serde_json::to_string(&profiles).unwrap()
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
    // JSC readline decodes bytes as Latin-1; transmit ASCII JSON escapes so source
    // accents and typographic punctuation retain their original code points.
    let encoded: String = serde_json::to_string(&script)
        .unwrap()
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_string()
            } else {
                c.encode_utf16(&mut [0; 2])
                    .iter()
                    .map(|u| format!("\\u{u:04x}"))
                    .collect()
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
    let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
    fn same(a: &Value, b: &Value, path: &str) {
        match (a, b) {
            (Value::Object(a), Value::Object(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (k, v) in a {
                    same(v, &b[k], &format!("{path}.{k}"));
                }
            }
            (Value::Array(a), Value::Array(b)) => {
                assert_eq!(a.len(), b.len(), "{path}");
                for (i, (a, b)) in a.iter().zip(b).enumerate() {
                    same(a, b, &format!("{path}[{i}]"));
                }
            }
            _ => assert_eq!(a, b, "{path}"),
        }
    }
    same(catalog(), &actual["catalog"], "catalog");
    assert_eq!(json!(expected), actual["outcomes"]);
}
