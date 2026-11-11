//! Rust-owned oracle for the original breath and rider weapon source. Only
//! presentation fields (colors and sound) are excluded; every Shot field is checked.
use dragon_game::combat::{Breath, Kind, Mouth, Path, Shot};
use serde_json::{Value, json};

fn breath(kind: Kind, stage: usize, skill: u8, follower: bool) -> Breath {
    Breath {
        kind,
        stage,
        skill,
        follower,
        owner: "fixture-dragon".into(),
        bellows: 2.0,
        dad: true,
    }
}
fn mouth() -> Mouth {
    Mouth {
        x: 123.0,
        y: 234.0,
        facing: -1.0,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}

#[test]
fn triple_breath_keeps_pattern_order_and_reduces_only_side_damage() {
    let b = breath(Kind::Flame, 2, 5, false);
    let shots = b.emit(mouth(), 40.0, [10.0, -20.0], || {
        panic!("flame consumes no random draws")
    });
    assert_eq!(shots.len(), 15);
    for (group, offset) in [0.0_f64, -0.22, 0.22].into_iter().enumerate() {
        for (index, angle) in [-0.4_f64, -0.2, 0.0, 0.2, 0.4].into_iter().enumerate() {
            let s = &shots[group * 5 + index];
            close(s.vx, -(angle + offset).cos() * 360.0 + 3.0);
            close(s.vy, (angle + offset).sin() * 360.0 - 3.0);
            close(s.dmg, 1.4 * 1.15 * if group == 0 { 1.0 } else { 0.6 });
            close(s.r, 40.0 * 0.16 * 1.2);
            assert!(s.dad);
            assert_eq!(s.owner, "fixture-dragon");
        }
    }
}

#[test]
fn follower_has_half_power_smaller_radius_no_dad_and_no_triple() {
    for kind in [
        Kind::Bond,
        Kind::Flame,
        Kind::Spark,
        Kind::Boulder,
        Kind::Frost,
        Kind::Star,
    ] {
        let main = breath(kind, 1, 4, false).emit(mouth(), 40.0, [0.0; 2], || 0.5);
        let follower = breath(kind, 1, 5, true).emit(mouth(), 40.0, [0.0; 2], || 0.5);
        assert_eq!(main.len(), follower.len());
        for (a, b) in main.iter().zip(&follower) {
            close(b.dmg, a.dmg * 0.5);
            close(b.r, a.r / 1.2 * 0.75);
            assert!(!b.dad);
        }
    }
}

#[test]
fn random_draw_count_and_bond_fan_are_exact() {
    for kind in [Kind::Spark, Kind::Star] {
        let mut draws = 0;
        let shots = breath(kind, 3, 5, false).emit(mouth(), 40.0, [0.0; 2], || {
            let value = [0.0, 0.5, 1.0][draws];
            draws += 1;
            value
        });
        assert_eq!(draws, 3);
        assert_eq!(shots.len(), 3);
    }
    for (stage, skill, follower, count) in [
        (0, 0, false, 1),
        (3, 0, false, 3),
        (0, 5, false, 3),
        (3, 5, false, 5),
        (3, 5, true, 3),
    ] {
        let shots =
            breath(Kind::Bond, stage, skill, follower)
                .emit(mouth(), 40.0, [0.0; 2], || panic!("bond consumes no draws"));
        assert_eq!(shots.len(), count);
        assert!(shots.windows(2).all(|s| s[0].vy < s[1].vy));
        assert!(shots.iter().all(|s| s.dmg == shots[0].dmg));
    }
}

fn source_block(start: &str, end: &str) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(start).expect("legacy block exists")..];
    &tail[..tail.find(end).expect("legacy block terminator exists")
        + if end.starts_with('\n') && !end.contains("function") {
            end.len()
        } else {
            0
        }]
}
fn compare(rust: &Value, js: &Value, label: &str) {
    match (rust, js) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() <= 1e-10 * b.abs().max(1.0),
                "{label}: Rust {a}, JS {b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{label} fields");
            for (k, v) in a {
                compare(v, &b[k], &format!("{label}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label} length");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{label}[{i}]"));
            }
        }
        _ => assert_eq!(rust, js, "{label}"),
    }
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_source_matches_all_breath_and_weapon_patterns() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for kind in [
        Kind::Bond,
        Kind::Flame,
        Kind::Spark,
        Kind::Boulder,
        Kind::Frost,
        Kind::Star,
    ] {
        for stage in 0..4 {
            for skill in 0..=5 {
                for follower in [false, true] {
                    for facing in [-1.0, 1.0] {
                        for dad in [false, true] {
                            let scale = if facing < 0.0 { 30.0 } else { 64.0 };
                            let b = Breath {
                                bellows: stage as f64,
                                dad,
                                ..breath(kind, stage, skill, follower)
                            };
                            let mut draws = 0;
                            let shots =
                                b.emit(Mouth { facing, ..mouth() }, scale, [31.0, -17.0], || {
                                    let value = [0.0, 0.5, 1.0][draws];
                                    draws += 1;
                                    value
                                });
                            cases.push(json!({"kind":kind,"stage":stage,"skill":skill,"follower":follower,"facing":facing,"dad":dad,"scale":scale,"bellows":b.bellows}));
                            expected
                                .push(json!({"shots":shots,"cooldown":b.cooldown(),"draws":draws}));
                        }
                    }
                }
            }
        }
    }
    for (name, path) in [
        ("shadow", Path::Shadow),
        ("forge", Path::Forge),
        ("storm", Path::Storm),
        ("guardian", Path::Guardian),
    ] {
        for facing in [-1.0, 1.0] {
            for dragon_scale in [0.62, 0.8, 1.0, 1.18] {
                for bellows in [0.0, 3.0] {
                    cases.push(json!({"path":name,"facing":facing,"dragon_scale":dragon_scale,"bellows":bellows,"scale":40.0}));
                    expected.push(json!({"shots":[path.weapon([123.0,234.0],facing,40.0,dragon_scale,bellows)],"cooldown":path.cooldown(),"draws":0}));
                }
            }
        }
    }
    let script = format!(
        r#"
const cases={cases}, defaults={defaults};
{kinds}
{stages}
{paths}
let S, P, R, shots, current, draws;
const stageOf=d=>current.stage, skillLv=()=>current.skill;
const bellowsMult=()=>1+0.2*current.bellows, isDaddio=()=>current.dad;
const scaleOf=()=>current.dragon_scale, bonded=()=>({{}});
const SHOT_COLORS={{}}, sfx={{breath:()=>{{}},weapon:()=>{{}}}};
const rand=(a,b)=>a+[0,0.5,1][draws++]*(b-a);
{cooldown}
{breath}
{weapon}
const results=cases.map(c=>{{current=c; draws=0; shots=[]; S=c.scale;
 P={{x:123,y:234,vx:31,vy:-17,facing:c.facing}}; R={{path:c.path,breath:[]}};
 let cd;
 if(c.path){{riderWeapon();cd=PATHS[c.path].cd;}}
 else{{const d={{kind:c.kind,id:'fixture-dragon',bonded:c.kind==='bond'}};
 breath(d,{{x:123,y:234,f:c.facing}},c.follower);cd=breathCd(d);}}
 return {{shots:shots.map(s=>{{const result={{...defaults,...s}};delete result.cols;result.hits=Array.from(result.hits);return result;}}),cooldown:cd,draws}};
}});
print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        defaults = serde_json::to_string(&Shot::default()).unwrap(),
        kinds = source_block("const KINDS = {", "\n};"),
        stages = source_block("const STAGES = [", "\n];"),
        paths = source_block("const PATHS = {", "\n};"),
        cooldown = source_block("function breathCd(", "\nfunction "),
        breath = source_block("function breath(", "\nfunction "),
        weapon = source_block("function riderWeapon(", "\nfunction ")
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
            .expect("JavaScriptCore CLI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "oracle failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
    assert_eq!(actual.len(), expected.len());
    for (i, (rust, js)) in expected.iter().zip(actual).enumerate() {
        compare(rust, &js, &format!("case {i}: {}", cases[i]));
    }
}
