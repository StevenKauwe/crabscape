//! Optional real-source oracle. Rust owns extraction, fixtures and comparison; no
//! separate JS/Node pipeline, browser, personal save, network or GUI is involved.
use dragon_game::flight::{FlightRules, Input, Player, Viewport};
use serde_json::{Value, json};

fn legacy_function(name: &str) -> &str {
    let source = include_str!("../../../web/dragon/index.html");
    let start = source
        .find(&format!("function {name}("))
        .expect("legacy function exists");
    let tail = &source[start..];
    &tail[..tail.find("\nfunction ").expect("following function exists")]
}
fn js_player(p: &Player) -> Value {
    let mut value = serde_json::to_value(p).unwrap();
    let fields = value.as_object_mut().unwrap();
    for (rust, js) in [
        ("special_cd", "specialCd"),
        ("fire_flash", "fireFlash"),
        ("main_cool", "mainCool"),
        ("weapon_cool", "weaponCool"),
        ("follow_cool", "followCool"),
        ("dad_cd", "dadCd"),
        ("mega_t", "megaT"),
    ] {
        let v = fields.remove(rust).unwrap();
        fields.insert(js.into(), v);
    }
    value
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() <= 1e-9 * b.abs().max(1.0),
                "{path}: Rust {a}, JS {b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                compare(v, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn real_legacy_source_matches_synthetic_frame_schedules() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for (screen, zoom, dragon_scale) in [
        ([390.0, 844.0], 1.0, 0.62),
        ([1366.0, 768.0], 1.6, 1.18),
        ([90.0, 120.0], 0.75, 0.8),
    ] {
        for mode in 0..7 {
            let view = Viewport::from_screen(screen[0], screen[1], zoom, 80.0, dragon_scale);
            let rules = FlightRules {
                tea: 3.0,
                magic: mode % 2 == 0,
                max_energy: 120.0,
                mission_done: mode == 6,
            };
            let mut p = Player {
                x: 200.0,
                y: 300.0,
                vx: 41.0,
                vy: -26.0,
                tired: 0.025,
                energy: 119.7,
                mega_t: 0.025,
                ..Player::default()
            };
            p.follow_cool.insert("friend".into(), 0.01);
            p.dad_cd.insert("dad".into(), 0.02);
            match mode {
                1 => p.out = true,
                2 => {
                    p.down = 0.08;
                    p.hearts = 0;
                }
                3 => p.dizzy = 0.08,
                4 => p.shadow = 0.08,
                5 => {
                    p.out = true;
                    p.down = 0.08;
                    p.dizzy = 0.08;
                    p.shadow = 0.08;
                }
                _ => {}
            }
            let initial = js_player(&p);
            // resize() clamps first, before the first update.
            view.clamp(&mut p);
            let mut frames = Vec::new();
            let mut states = Vec::new();
            for frame in 0..160 {
                let dt = [0.0, 1.0 / 120.0, 1.0 / 60.0, 0.05, 0.2, -0.01, 0.013][frame % 7];
                let input = Input {
                    right: frame % 4 == 0,
                    left: frame % 4 == 1,
                    up: frame % 3 == 0,
                    down: frame % 3 == 1,
                    fire: true,
                    stick: match frame % 8 {
                        0 => Some([0.12, 0.0]),
                        1 => Some([-0.15, 0.0]),
                        2 => Some([0.16, 0.0]),
                        3 => Some([-2.0, 3.0]),
                        4 => Some([0.0, 0.0]),
                        _ => None,
                    },
                };
                frames.push(json!({"dt":dt,"input":input}));
                let event = p.step(dt, input, view, rules).unwrap();
                states.push(json!({"p":js_player(&p), "events":{"mega":event.mega_blast,"shadow":event.shadow_burst,"recover":event.recovered,"shoot":event.shoot}}));
            }
            cases.push(json!({"screen":screen,"zoom":zoom,"scale":dragon_scale,"rules":rules,"p":initial,"frames":frames}));
            expected.push(json!({"view":{"width":view.width,"height":view.height,"scale":view.scale,"ground_y":view.ground_y,"top_limit":view.top_limit},"states":states}));
        }
    }
    let script = format!(
        r#"
const cases = {cases};
const clamp = (x,a,b) => Math.max(a,Math.min(b,x));
const TAU = Math.PI*2, rand = (a,b) => (a+b)/2;
Math.random = () => 1; // disable only cosmetic particles
let P, W, H, S, groundY, topLimit, DPR, VIEW, stars, window, rules, input, events, size;
const cv = {{}}, hud = {{getBoundingClientRect:()=>({{bottom:80}})}}, me = {{}};
const bonded = () => ({{}}), scaleOf = () => size, paintBackground = () => {{}};
const teaLv = () => rules.tea, skillLv = () => rules.magic ? 1 : 0, maxEnergy = () => rules.max_energy;
const held = key => input[key], stickRadius = () => 1;
const megaBlast = () => events.mega = true, shadowBurst = () => events.shadow = true;
const shootAll = () => events.shoot = true, say = () => events.recover = true, updateHud = () => {{}};
const R = {{dragon:'synthetic'}}, particles = []; let mission;
{resize}
{clamp_player}
{update_player}
const results = cases.map(c => {{
 P=c.p; VIEW=c.zoom; size=c.scale; rules=c.rules; mission={{done:rules.mission_done}};
 window={{innerWidth:c.screen[0],innerHeight:c.screen[1],devicePixelRatio:1}}; resize();
 const view={{width:W,height:H,scale:S,ground_y:groundY,top_limit:topLimit}};
 const states=c.frames.map(f=>{{ input=f.input; events={{mega:false,shadow:false,recover:false,shoot:false}};
 P.stick=input.stick ? {{x0:0,y0:0,x:input.stick[0],y:input.stick[1]}} : null;
 updatePlayer(Math.min(0.05,Math.max(0,f.dt)));
 return {{p:JSON.parse(JSON.stringify(P)),events}};
 }}); return {{view,states}};
}}); print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        resize = legacy_function("resize"),
        clamp_player = legacy_function("clampPlayer"),
        update_player = legacy_function("updatePlayer")
    );
    // Pipe the source into JSC rather than exceeding the OS argument-size limit.
    use std::io::Write;
    use std::process::Stdio;
    let mut child = std::process::Command::new(
        "/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc",
    )
    .args(["-e", "eval(JSON.parse(readline()))"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("macOS JavaScriptCore CLI is required");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "JavaScript oracle failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&output.stdout).expect("oracle JSON");
    assert_eq!(actual.len(), expected.len());
    for (case, (rust, js)) in expected.iter().zip(&actual).enumerate() {
        compare(&rust["view"], &js["view"], &format!("case {case} viewport"));
        let rust_states = rust["states"].as_array().unwrap();
        let js_states = js["states"].as_array().unwrap();
        assert_eq!(rust_states.len(), js_states.len());
        for (frame, (rust, js)) in rust_states.iter().zip(js_states).enumerate() {
            compare(rust, js, &format!("case {case} frame {frame}"));
        }
    }
}
