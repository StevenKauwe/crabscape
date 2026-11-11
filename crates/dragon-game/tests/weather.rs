//! Recorded-random live-source oracle, independent of gameplay and GUI state.
use dragon_game::{
    drawing::{Canvas, Weather, WeatherFrame},
    flight::Viewport,
};
use serde_json::{Value, json};
fn function(name: &str) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(&format!("function {name}(")).unwrap()..];
    let open = tail.find(") {").unwrap() + 2;
    let mut depth = 0;
    for (i, ch) in tail.char_indices().skip_while(|(i, _)| *i < open) {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                return &tail[..=i];
            }
        }
    }
    panic!("function");
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() < 1e-8 * b.abs().max(1.), "{path}: {a} != {b}");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, v) in a {
                compare(v, &b[k], &format!("{path}.{k}"));
            }
        }
        (Value::String(a), Value::String(b))
            if a.starts_with("rgba(") && b.starts_with("rgba(") =>
        {
            let parse = |s: &str| {
                s[5..s.len() - 1]
                    .split(',')
                    .map(|v| v.parse::<f64>().unwrap())
                    .collect::<Vec<_>>()
            };
            compare(&json!(parse(a)), &json!(parse(b)), path);
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn view() -> Viewport {
    Viewport {
        width: 1000.,
        height: 700.,
        scale: 48.,
        ground_y: 600.,
        top_limit: 80.,
        dragon_scale: 1.,
    }
}
#[test]
fn weather_caps_resets_and_keeps_nonfinite_source_twinkles_out_of_gpu_geometry() {
    let mut w = Weather::default();
    let mut rng = || 0.;
    for _ in 0..200 {
        w.update(
            0.001,
            WeatherFrame {
                view: view(),
                realm: 2,
                time: 0.,
                scroll_v: 0.,
                fancy: true,
                in_sky: true,
            },
            &mut rng,
        );
    }
    assert_eq!(w.ambient.len(), 90);
    w.update(
        0.,
        WeatherFrame {
            view: view(),
            realm: 2,
            time: 0.,
            scroll_v: 0.,
            fancy: false,
            in_sky: true,
        },
        &mut rng,
    );
    assert!(w.ambient.is_empty());
    w.update(
        0.01,
        WeatherFrame {
            view: view(),
            realm: 5,
            time: 0.,
            scroll_v: 0.,
            fancy: true,
            in_sky: true,
        },
        &mut rng,
    );
    assert!(
        w.ambient
            .iter()
            .any(|a| a.kind == "twinkle" && a.x.is_nan() && a.y.is_nan())
    );
    let mut c = Canvas::default();
    w.draw_ambient(&mut c, 48., 0.);
    assert!(
        c.commands
            .iter()
            .all(|d| d.transform.0.iter().all(|n| n.is_finite()))
    );
    for _ in 0..600 {
        w.dad_trail([100., 200.], 48., 1., 48., true, &mut rng);
    }
    assert_eq!(w.trails.len(), 512);
    w.update(
        0.7,
        WeatherFrame {
            view: view(),
            realm: 0,
            time: 0.,
            scroll_v: 0.,
            fancy: false,
            in_sky: false,
        },
        &mut rng,
    );
    assert!(w.trails.is_empty());
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; source weather + trails recorded RNG"]
fn weather_and_trails_match_original_source_with_recorded_randomness() {
    let harness = include_str!("drawing.rs");
    let tail = &harness[harness.find("const TAU=Math.PI*2;").unwrap()..];
    let mut script = tail[..tail.find("\"#,\n").unwrap()].to_owned();
    script.push_str("\nlet state='play',fancy=true,reduceMotion=false;const ambient=[],particles=[];let n=0;Math.random=()=>((n++*37)%997)/997;const pick=a=>a[Math.floor(Math.random()*a.length)];const skyBottom=()=>groundY-S*([1,2,4].includes(realmIdx)?2.6:0.5);\n");
    // The shared command recorder's rand is constant in geometry fixtures; this oracle needs the tape.
    script = script.replace(
        "const rand=(a,b)=>a+(b-a)*0.4,",
        "const rand=(a,b)=>a+(b-a)*Math.random(),",
    );
    for name in [
        "circle",
        "ell",
        "drawStar",
        "updateAmbient",
        "drawAmbient",
        "dadTrail",
    ] {
        script.push_str(function(name));
        script.push('\n');
    }
    script.push_str("const oldTranslate=Recorder.prototype.translate;Recorder.prototype.translate=function(x,y){if(Number.isFinite(x)&&Number.isFinite(y))oldTranslate.call(this,x,y);};\n");
    script.push_str(r#"
const normalize=a=>({kind:a.kind,x:a.x,y:a.y,vx:a.vx===undefined?NaN:a.vx,vy:a.vy===undefined?NaN:a.vy,life:a.life,max:a.max,ph:a.ph||0,r:a.r||0,rot:a.rot||0,vr:a.vr||0,g:a.g||0,color:a.color||''});
const result=[];
REALMS=['hollow','wood','frost','sea','ember','stars'].map(id=>({id}));
for(realmIdx=0;realmIdx<6;realmIdx++){
 ambient.length=0;particles.length=0;n=0;
 for(let i=0;i<240;i++){
  const dt=[0,1/60,0.1,0.4][i%4];time=i*0.03;scrollV=i%3?123:0;
  for(let j=particles.length-1;j>=0;j--){const p=particles[j];p.life-=dt;if(p.life<=0){particles.splice(j,1);continue;}p.x+=p.vx*dt;p.y+=p.vy*dt;}
  updateAmbient(dt);dadTrail(300,200,48,i<120?1:-1);
  if(i%30===0||i===239){ctx=new Recorder();drawAmbient();const weather=ctx.commands;ctx=new Recorder();for(const p of particles){const a=Math.max(0,p.life/p.max);ctx.globalCompositeOperation='lighter';ctx.globalAlpha=a;ctx.fillStyle=p.color;circle(p.x,p.y,p.r*a+0.5);}result.push({ambient:ambient.map(normalize),trails:particles.map(normalize),n,weather,trailDraw:ctx.commands});}
 }
}
print(JSON.stringify(result));
"#);
    let mut expected = Vec::new();
    for realm in 0..6 {
        let mut w = Weather::default();
        let mut n = 0;
        for i in 0..240 {
            let mut rng = || {
                let v = ((n * 37) % 997) as f64 / 997.;
                n += 1;
                v
            };
            let time = i as f64 * 0.03;
            let dt = [0., 1. / 60., 0.1, 0.4][i % 4];
            w.update(
                dt,
                WeatherFrame {
                    view: view(),
                    realm,
                    time,
                    scroll_v: if i % 3 != 0 { 123. } else { 0. },
                    fancy: true,
                    in_sky: true,
                },
                &mut rng,
            );
            w.dad_trail(
                [300., 200.],
                48.,
                if i < 120 { 1. } else { -1. },
                48.,
                true,
                &mut rng,
            );
            if i % 30 == 0 || i == 239 {
                let mut c = Canvas::default();
                w.draw_ambient(&mut c, 48., time);
                let mut t = Canvas::default();
                w.draw_trails(&mut t);
                expected.push(json!({"ambient":w.ambient,"trails":w.trails,"n":n,"weather":c.commands,"trailDraw":t.commands}));
            }
        }
    }
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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", json!(script)).as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let actual: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    compare(&json!(expected), &json!(actual), "weather");
}

#[test]
fn fireflies_cap_at_fourteen_and_disabled_effects_do_not_consume_randomness() {
    let mut w = Weather::default();
    let mut draws = 0;
    for _ in 0..30 {
        w.update(
            0.001,
            WeatherFrame {
                view: view(),
                realm: 1,
                time: 0.,
                scroll_v: 0.,
                fancy: true,
                in_sky: true,
            },
            &mut || {
                draws += 1;
                0.
            },
        );
    }
    assert_eq!(w.ambient.iter().filter(|a| a.kind == "fly").count(), 14);
    let before = draws;
    w.dad_trail([0., 0.], 48., 1., 48., false, &mut || {
        draws += 1;
        0.
    });
    w.update(
        0.1,
        WeatherFrame {
            view: view(),
            realm: 1,
            time: 0.,
            scroll_v: 0.,
            fancy: false,
            in_sky: true,
        },
        &mut || {
            draws += 1;
            0.
        },
    );
    assert_eq!(draws, before);
    assert!(w.ambient.is_empty());
    assert!(w.trails.is_empty());
}
