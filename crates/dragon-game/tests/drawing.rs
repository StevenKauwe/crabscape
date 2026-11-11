//! Compare geometry, layer order, colors, gradients and affine transforms to real source.
//! The optional macOS oracle uses only synthetic inputs and a recording Canvas, never a GUI.
use dragon_game::drawing::*;
use serde_json::{Value, json};
#[test]
fn shade_preserves_source_hex_only_quirk() {
    assert_eq!(shade("#804020", 0.5), "rgb(192,160,144)");
    assert_eq!(shade("rgb(128,64,32)", 0.3), "rgb(77,77,77)");
    assert_eq!(source_rgb("#fff"), [0, 15, 255]);
}
#[test]
fn canvas_restores_state_but_not_current_path() {
    let mut c = Canvas::default();
    c.save();
    c.translate(2., 3.);
    c.begin();
    c.move_to(1., 1.);
    c.restore();
    c.line_to(4., 5.);
    c.fill();
    assert_eq!(c.commands[0].transform, Affine::default());
    assert_eq!(
        c.commands[0].path,
        vec![PathCommand::Move(1., 1.), PathCommand::Line(4., 5.)]
    );
}
#[test]
fn canvas_frame_reset_keeps_outer_buffer_and_resets_style() {
    let mut c = Canvas::default();
    c.alpha(0.3);
    c.translate(20., 30.);
    c.fill_rect(0., 0., 10., 10.);
    let capacity = c.commands.capacity();
    c.reset();
    assert!(c.commands.is_empty());
    assert_eq!(c.commands.capacity(), capacity);
    c.fill_rect(0., 0., 10., 10.);
    assert_eq!(c.commands[0].alpha, 1.);
    assert_eq!(c.commands[0].transform, Affine::default());
}
#[test]
fn dragon_options_retain_clip_and_rider_layers() {
    let mut c = Canvas::default();
    draw_dragon(
        &mut c,
        0.,
        0.,
        48.,
        "#3f5ad0",
        -1.,
        1.25,
        &DragonArt {
            dad: true,
            rider: Some(RiderArt {
                crown: true,
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert!(c.commands.iter().any(|d| !d.clips.is_empty()));
    assert!(c.commands.len() > 80);
    let mut plain = Canvas::default();
    draw_dragon(
        &mut plain,
        0.,
        0.,
        48.,
        "#3f5ad0",
        1.,
        1.25,
        &DragonArt::default(),
    );
    assert!(plain.commands.len() < c.commands.len());
    assert!(
        plain
            .commands
            .iter()
            .any(|d| matches!(d.paint, Paint::Radial { .. }))
    );
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() < 1e-8 * b.abs().max(1.),
                "{path}: Rust {a} JS {b}"
            );
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: {a:?} vs {b:?}");
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
                    .map(|s| s.parse::<f64>().unwrap())
                    .collect::<Vec<_>>()
            };
            compare(&json!(parse(a)), &json!(parse(b)), path);
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn source_function(name: &str) -> &str {
    let src = include_str!("../../../web/dragon/index.html");
    let start = src.find(&format!("function {name}(")).unwrap();
    let tail = &src[start..];
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
    panic!("missing function end {name}")
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_canvas_command_streams_match() {
    let mut inputs = Vec::new();
    let mut rust = Vec::new();
    for i in 0..24 {
        let t = i as f64 * 0.137;
        let o = DragonArt {
            rider_time: Some(2.4),
            aura: i % 3 == 0,
            dad: i % 4 == 0,
            dizzy: i % 5 == 0,
            spin: if i % 2 == 0 { 0.7 } else { 0. },
            fire: i % 2 == 1,
            horn: Some("#ff9fcf".into()),
            eye: if i % 2 == 0 {
                Some("#102030".into())
            } else {
                None
            },
            rider: Some(RiderArt {
                style: ["short", "long", "braid", "curly", "hood"][i % 5].into(),
                path: ["shadow", "forge", "storm", "guardian"][i % 4].into(),
                crown: i % 2 == 0,
                daddio: i % 4 == 0,
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = if i % 2 == 0 { 30. } else { 64. };
        let f = if i % 3 == 0 { -1. } else { 1. };
        let mut c = Canvas::default();
        draw_dragon(&mut c, 230., 180., s, "#3f5ad0", f, t, &o);
        rust.push(json!(c.commands));
        let mut options = json!(o);
        options["fireCols"] = options["fire_cols"].take();
        inputs.push(json!({"dragon":true,"s":s,"f":f,"t":t,"o":options}));
    }
    let scene = SceneArt {
        player: [300., 190.],
        ground: 600.,
        scale: 48.,
        time: 2.4,
    };
    for kind in [
        "cloud",
        "bigcloud",
        "thunderhead",
        "bat",
        "sprite",
        "urgal",
        "chieftain",
        "wolf",
        "troll",
        "cavetroll",
        "jelly",
        "leaper",
        "serpent",
        "slime",
        "slimelet",
        "bigslime",
        "imp",
        "gobbler",
        "moth",
        "swallower",
    ] {
        for mode in 0..5 {
            let e = EnemyArt {
                kind: kind.into(),
                x: 220.,
                y: 350.,
                r: 23.,
                dir: if mode % 2 == 0 { -1. } else { 1. },
                t: 0.73,
                wob: 0.4,
                flash: if mode == 1 { 0.2 } else { 0. },
                calm: mode == 2,
                slow: if mode == 3 { 0.3 } else { 0. },
                windup: if mode == 4 { 0.7 } else { 0. },
                stun: 0.2,
                air: mode % 2 == 0,
                vy: 37.,
                hop: kind.contains("slime"),
            };
            let mut c = Canvas::default();
            draw_enemy(&mut c, &e, &scene);
            rust.push(json!(c.commands));
            let mut e = json!(e);
            e["type"] = e["kind"].take();
            e["def"] = json!({"hop":e["hop"]});
            inputs.push(json!({"dragon":false,"e":e}));
        }
    }
    for realm in ["hollow", "sea", "ember"] {
        for kind in [
            "rock", "boulder", "ember", "bubble", "spark", "dust", "fall",
        ] {
            for warn in [0., 0.7] {
                let h = dragon_game::damage::Hazard {
                    x: 140.,
                    y: 250.,
                    r: 12.,
                    spin: 0.4,
                    warn,
                    kind: kind.into(),
                    ..Default::default()
                };
                let mut c = Canvas::default();
                draw_hazard(&mut c, &h, &scene, realm);
                rust.push(json!(c.commands));
                inputs.push(json!({"hazard":h,"realm":realm}));
            }
        }
    }
    for warn in [0., 0.7] {
        let st = dragon_game::damage::Strike {
            x: 170.,
            y0: Some(120.),
            warn,
            ..Default::default()
        };
        let mut c = Canvas::default();
        draw_strike(&mut c, &st, &scene, 80., || 0.4);
        rust.push(json!(c.commands));
        inputs.push(json!({"strike":st}));
    }
    for what in ["barn", "fawn", "goat", "lamp", "boats", "garden", "star"] {
        for done in [false, true] {
            let ob:dragon_game::objectives::Objective=serde_json::from_value(json!({"kind":"protect","what":what,"name":"fixture","x":230.,"y":510.,"x0":230.,"to":650.,"dur":40.,"t":0.73,"hp":3,"maxHp":5,"carrier":-1,"done":done,"vx":0.,"vy":0.,"stealCd":0.,"hurtT":0.2,"r":20.})).unwrap();
            let mut c = Canvas::default();
            draw_objective(&mut c, &ob, &scene);
            rust.push(json!(c.commands));
            inputs.push(json!({"objective":ob}));
        }
    }
    for mode in 0..8 {
        let e = EnemyArt {
            kind: "bat".into(),
            x: 150.,
            y: 200.,
            r: 24.,
            dir: 1.,
            t: 0.3,
            flash: 0.08,
            calm: mode == 7,
            ..Default::default()
        };
        let mut c = Canvas::default();
        draw_enemy_fx(
            &mut c,
            &e,
            &scene,
            0.2,
            if mode & 1 != 0 { 0.2 } else { 0. },
            if mode & 2 != 0 { 0.2 } else { 0. },
            mode & 4 != 0,
            0.,
        );
        rust.push(json!(c.commands));
        let mut js = json!(e);
        js["type"] = json!("bat");
        js["sq"] = json!(0.2);
        js["pre"] = json!(if mode & 1 != 0 { 0.2 } else { 0. });
        js["lunge"] = json!(if mode & 2 != 0 { 0.2 } else { 0. });
        js["hurt"] = json!(mode & 4 != 0);
        js["sting"] = json!(0.);
        inputs.push(json!({"fx":js}));
        let mut c = Canvas::default();
        draw_warning(&mut c, &e, mode % 2 == 0, &scene);
        rust.push(json!(c.commands));
        inputs.push(json!({"warning":js,"hot":mode%2==0}));
    }
    for kind in [
        "bond", "flame", "spark", "boulder", "frost", "star", "arrow", "hammer", "area",
    ] {
        for mode in 0..4 {
            let shot = dragon_game::combat::Shot {
                x: 230.,
                y: 210.,
                vx: -130.,
                vy: 24.,
                r: 9.,
                spin: 0.3,
                kind: serde_json::from_value(json!(kind)).unwrap(),
                dad: mode == 1,
                spear: mode == 2,
                ..Default::default()
            };
            let cols = (mode == 3).then(|| ["#ffffff".into(), "#abcdef".into(), "#aabbcc".into()]);
            let mut c = Canvas::default();
            draw_shot(&mut c, &shot, cols.as_ref(), &scene);
            rust.push(json!(c.commands));
            let mut js = json!(shot);
            js["cols"] = json!(cols);
            inputs.push(json!({"shot":js}));
        }
    }
    for golden in [false, true] {
        for life in [1., 3.] {
            let gem = dragon_game::gems::Gem {
                x: 210.,
                y: 140.,
                r: 10.,
                t: 0.3,
                golden,
                life,
                ..Default::default()
            };
            let mut c = Canvas::default();
            draw_gem(&mut c, &gem, "#c79bff", &scene);
            rust.push(json!(c.commands));
            let mut js = json!(gem);
            js["color"] = json!("#c79bff");
            inputs.push(json!({"gem":js}));
        }
    }
    for realm in 0..6 {
        for song in [false, true] {
            let view = dragon_game::flight::Viewport {
                width: 1000.,
                height: 700.,
                scale: 48.,
                ground_y: 600.,
                top_limit: 80.,
                dragon_scale: 0.62,
            };
            let b = draw_background(view, realm, song);
            rust.push(json!([b.sky.commands, b.far.commands, b.land.commands]));
            inputs.push(json!({"background":true,"realm":realm,"song":song}));
        }
    }
    for realm in 0..6 {
        for scroll in [0., 1234., 9999.] {
            let view = dragon_game::flight::Viewport {
                width: 1000.,
                height: 700.,
                scale: 48.,
                ground_y: 600.,
                top_limit: 80.,
                dragon_scale: 0.62,
            };
            let mut c = Canvas::default();
            draw_scenery(&mut c, view, realm, scroll, 123.);
            rust.push(json!(c.commands));
            inputs.push(json!({"scenery":true,"realm":realm,"scroll":scroll}));
        }
    }
    let view = dragon_game::flight::Viewport {
        width: 1000.,
        height: 700.,
        scale: 48.,
        ground_y: 600.,
        top_limit: 80.,
        dragon_scale: 0.62,
    };
    for realm in 0..6 {
        for scroll in [0., 1234., 9999.] {
            let mut c = Canvas::default();
            draw_foreground(&mut c, view, realm, scroll, 2.4);
            rust.push(json!(c.commands));
            inputs.push(json!({"foreground":true,"realm":realm,"scroll":scroll}));
        }
    }
    let mut c = Canvas::default();
    draw_moon_rays(&mut c, view, 2.4);
    rust.push(json!(c.commands));
    inputs.push(json!({"moon":true}));
    for n in [0, 3, 20] {
        let shots: Vec<_> = (0..n)
            .map(|i| dragon_game::combat::Shot {
                x: 100. + i as f64 * 30.,
                y: 400. + i as f64 * 20.,
                kind: dragon_game::combat::Kind::Flame,
                ..Default::default()
            })
            .collect();
        let mut c = Canvas::default();
        draw_shot_lights(&mut c, view, &shots);
        rust.push(json!(c.commands));
        inputs.push(json!({"lights":shots}));
    }
    let mut script = String::from(
        r#"
const TAU=Math.PI*2; let time=2.4, P={x:300,y:190}, groundY=600,S=48,ctx,realmIdx=0,REALMS=[{id:'hollow'}],topLimit=80;
const rand=(a,b)=>a+(b-a)*0.4,nameTag=()=>{},placeName=x=>x,hurtsNow=e=>e.hot;
let shots=[],skyFlash=0;
const SHOT_COLORS={flame:['#fff6c2','#ffb347','#ff5a1f']};
let W=1000,H=700,DPR=1,VIEW=1,me=true,song=false,bg={},land={},far={},cv={width:1000,height:700},bgx,landx,farx; const msReached=()=>song;
let scroll=0,scrollV=123;const FLY_SPEED=4.2,hash01=(i,k)=>{const v=Math.sin(i*127.1+k*311.7)*43758.5453;return v-Math.floor(v);};
const dist=(a,b,c,d)=>Math.hypot(a-c,b-d),clamp=(x,a,b)=>Math.max(a,Math.min(b,x)),isDaddio=r=>r.daddio,safeColor=(c,f)=>/^#[0-9a-f]{6}$/i.test(c)?c:f;
class Recorder {
 constructor(){this.commands=[];this.path=[];this.stack=[];this.s={transform:[1,0,0,1,0,0],fillStyle:{Solid:'#000000'},strokeStyle:{Solid:'#000000'},globalAlpha:1,lineWidth:1,lineCap:'butt',lineJoin:'miter',lineDashOffset:0,dash:[],globalCompositeOperation:'source-over',clips:[]};}
 save(){this.stack.push(JSON.parse(JSON.stringify(this.s)));} restore(){this.s=this.stack.pop();}
 transform(g,h,i,j,k,l){let[a,b,c,d,e,f]=this.s.transform;this.s.transform=[a*g+c*h,b*g+d*h,a*i+c*j,b*i+d*j,a*k+c*l+e,b*k+d*l+f];}
 setTransform(...v){this.s.transform=v;}
 translate(x,y){this.transform(1,0,0,1,x,y);}scale(x,y){this.transform(x,0,0,y,0,0);}rotate(a){this.transform(Math.cos(a),Math.sin(a),-Math.sin(a),Math.cos(a),0,0);}
 bezierCurveTo(...v){this.path.push({Cubic:v});}roundRect(...v){this.path.push({RoundRect:v});}beginPath(){this.path=[];}moveTo(...v){this.path.push({Move:v});}lineTo(...v){this.path.push({Line:v});}quadraticCurveTo(...v){this.path.push({Quad:v});}ellipse(...v){this.path.push({Ellipse:v});}arc(x,y,r,a,b){this.ellipse(x,y,r,r,0,a,b);}closePath(){this.path.push('Close');}
 setLineDash(v){this.s.dash=v.slice();}
 fillText(){}strokeText(){}
 strokeRect(x,y,w,h){let old=this.path;this.beginPath();this.moveTo(x,y);this.lineTo(x+w,y);this.lineTo(x+w,y+h);this.lineTo(x,y+h);this.closePath();this.stroke();this.path=old;}
 clip(){this.s.clips.push({path:this.path.slice(),transform:this.s.transform.slice()});}
 emit(stroke){this.commands.push(JSON.parse(JSON.stringify({path:this.path,transform:this.s.transform,paint:this.s[stroke?'strokeStyle':'fillStyle'],alpha:this.s.globalAlpha,additive:this.s.globalCompositeOperation==='lighter',stroke:stroke?{width:this.s.lineWidth,cap:this.s.lineCap,join:this.s.lineJoin,dash:this.s.dash,...(this.s.lineDashOffset?{dash_offset:this.s.lineDashOffset}:{})}:null,clips:this.s.clips})));}
 fill(){this.emit(false);}stroke(){this.emit(true);}fillRect(x,y,w,h){let old=this.path;this.beginPath();this.moveTo(x,y);this.lineTo(x+w,y);this.lineTo(x+w,y+h);this.lineTo(x,y+h);this.closePath();this.fill();this.path=old;}
 gradient(kind,coords){let v={coords,transform:this.s.transform.slice(),stops:[]};let p={[kind]:v};p.addColorStop=(x,c)=>v.stops.push([x,c]);return p;}
 createLinearGradient(...v){return this.gradient('Linear',v);}createRadialGradient(...v){return this.gradient('Radial',v);}
}
for(let k of ['fillStyle','strokeStyle','globalAlpha','lineWidth','lineCap','lineJoin','lineDashOffset','globalCompositeOperation'])Object.defineProperty(Recorder.prototype,k,{get(){return this.s[k]},set(v){this.s[k]=typeof v==='string'&&(k==='fillStyle'||k==='strokeStyle')?{Solid:v}:v}});
"#,
    );
    for name in [
        "rgb",
        "shade",
        "hexA",
        "ell",
        "tri",
        "circle",
        "drawStar",
        "wing",
        "hairBack",
        "hairTop",
        "drawGear",
        "drawCrown",
        "drawRider",
        "drawDragon",
        "bodyColor",
        "drawCloudE",
        "drawBat",
        "drawSprite",
        "drawUrgal",
        "drawWolf",
        "drawTroll",
        "blobFace",
        "drawJelly",
        "drawLeaper",
        "drawSerpent",
        "drawSlime",
        "drawImp",
        "drawGobbler",
        "drawMoth",
        "drawSwallower",
        "drawEnemy",
        "drawEnemyFx",
        "drawWarn",
        "drawHazard",
        "drawStrike",
        "drawObjective",
        "drawShot",
        "drawGem",
        "mulberry",
        "paintBackground",
        "paintFarRealm",
        "paintConstellation",
        "drawScenery",
        "drawMoonRays",
        "drawForeground",
        "drawLights",
    ] {
        script.push_str(source_function(name));
        script.push('\n');
    }
    script.push_str(&format!("const cases={};print(JSON.stringify(cases.map(v=>{{ctx=new Recorder();if(v.background){{bgx=new Recorder();farx=new Recorder();landx=new Recorder();REALMS=['hollow','wood','frost','sea','ember','stars'].map(id=>({{id}}));realmIdx=v.realm;song=v.song;paintBackground();realmIdx=0;return [bgx.commands,farx.commands,landx.commands];}}if(v.scenery){{REALMS=['hollow','wood','frost','sea','ember','stars'].map(id=>({{id}}));realmIdx=v.realm;scroll=v.scroll;drawScenery();realmIdx=0;return ctx.commands;}}if(v.foreground){{REALMS=['hollow','wood','frost','sea','ember','stars'].map(id=>({{id}}));realmIdx=v.realm;scroll=v.scroll;drawForeground();realmIdx=0;return ctx.commands;}}if(v.moon){{drawMoonRays();return ctx.commands;}}if(v.lights){{shots=v.lights;drawLights();return ctx.commands;}}if(v.dragon)drawDragon(ctx,230,180,v.s,'#3f5ad0',v.f,v.t,v.o);else if(v.shot)drawShot(v.shot);else if(v.gem)drawGem(v.gem);else if(v.hazard){{REALMS[0].id=v.realm;drawHazard(v.hazard);}}else if(v.strike)drawStrike(v.strike);else if(v.objective)drawObjective(v.objective);else if(v.fx)drawEnemyFx(v.fx);else if(v.warning){{v.warning.hot=v.hot;drawWarn(v.warning);}}else drawEnemy(v.e);return ctx.commands;}})));",json!(inputs)));
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
            .expect("JSC required");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", json!(script)).as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let js: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rust.len(), js.len());
    for (i, (rust, js)) in rust.iter().zip(&js).enumerate() {
        compare(rust, js, &format!("case {i}"));
    }
}
