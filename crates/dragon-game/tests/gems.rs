//! Real-source geometry oracle. Synthetic unique values identify gems; collection
//! records those values only, without applying rewards/progression or save writes.
use dragon_game::flight::{Player, Viewport};
use dragon_game::gems::{Gem, update_gems};
use serde_json::{Value, json};
fn view() -> Viewport {
    Viewport {
        width: 800.0,
        height: 700.0,
        scale: 40.0,
        ground_y: 600.0,
        top_limit: 40.0,
        dragon_scale: 1.0,
    }
}
fn player() -> Player {
    Player {
        x: 100.0,
        y: 100.0,
        ..Player::default()
    }
}
fn gem(value: f64) -> Gem {
    Gem {
        x: 100.0,
        y: 100.0,
        r: 5.0,
        life: 1.0,
        value,
        ..Gem::default()
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10, "{a} != {b}");
}
#[test]
fn position_integrates_before_damping_and_inverted_bounds_match_js() {
    let mut gems = vec![Gem {
        x: 400.0,
        y: 300.0,
        vx: 100.0,
        vy: -80.0,
        ..gem(1.0)
    }];
    update_gems(&mut gems, 0.05, &player(), view(), false, &[], |_| {
        panic!("distant gem")
    });
    close(gems[0].x, 405.0);
    close(gems[0].y, 296.0);
    close(gems[0].vx, 87.5);
    close(gems[0].vy, -70.0);
    let tiny = Viewport {
        width: 4.0,
        height: 700.0,
        ground_y: 41.0,
        ..view()
    };
    update_gems(&mut gems, 0.0, &player(), tiny, false, &[], |_| {
        panic!("distant gem")
    });
    close(gems[0].x, 5.0);
    close(gems[0].y, 45.0);
}
#[test]
fn expiry_magnet_sweep_and_zero_distance_order() {
    for (magnet, sweep, collected) in [(0.0, false, false), (1.0, false, true), (0.0, true, true)] {
        let p = Player { magnet, ..player() };
        let mut gems = vec![Gem {
            life: 0.01,
            ..gem(1.0)
        }];
        let mut values = Vec::new();
        update_gems(&mut gems, 0.02, &p, view(), sweep, &[], |g| {
            close(g.vx, 0.0);
            close(g.vy, 0.0);
            close(g.life, 1.0);
            values.push(g.value);
        });
        assert!(gems.is_empty());
        assert_eq!(!values.is_empty(), collected);
    }
    for (sweep, speed) in [(false, 360.0), (true, 720.0)] {
        let p = Player {
            magnet: 1.0,
            ..player()
        };
        let mut gems = vec![Gem {
            x: 400.0,
            ..gem(1.0)
        }];
        update_gems(&mut gems, 0.02, &p, view(), sweep, &[], |_| {
            panic!("distant gem")
        });
        close(gems[0].vx, -speed);
        close(gems[0].life, 1.0);
    }
}
#[test]
fn follower_and_lost_gems_respect_reverse_order_and_strict_reach() {
    let mut gems = vec![
        gem(1.0),
        Gem {
            x: 400.0,
            ..gem(2.0)
        },
        Gem {
            lost_one: true,
            ..gem(3.0)
        },
        gem(4.0),
    ];
    let mut values = Vec::new();
    update_gems(
        &mut gems,
        0.0,
        &player(),
        view(),
        false,
        &[[400.0, 100.0]],
        |g| values.push(g.value),
    );
    assert!(gems.is_empty());
    assert_eq!(values, vec![4.0, 2.0, 1.0]);
    let p = Player {
        x: 0.0,
        y: 100.0,
        ..player()
    };
    let v = Viewport {
        scale: 100.0,
        ..view()
    };
    let mut gems = vec![Gem {
        x: 67.0,
        ..gem(1.0)
    }];
    update_gems(&mut gems, 0.0, &p, v, false, &[], |_| {
        panic!("exact edge must not collect")
    });
    assert_eq!(gems.len(), 1);
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().unwrap();
            let b = b.as_f64().unwrap();
            assert!(
                (a - b).abs() <= 1e-10 * b.abs().max(1.0),
                "{path}: Rust {a}, JS {b}"
            );
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, v) in a {
                compare(v, &b[k], &format!("{path}.{k}"));
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_update_gems_matches_synthetic_frames() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for magnet in [0.0, 1.0] {
        for mission in 0..4 {
            for tiny in [false, true] {
                for followers_on in [false, true] {
                    for dt in [0.0, 1.0 / 120.0, 0.05, 0.5] {
                        let p = Player { magnet, ..player() };
                        let v = if tiny {
                            Viewport {
                                width: 4.0,
                                height: 700.0,
                                ground_y: 41.0,
                                ..view()
                            }
                        } else {
                            view()
                        };
                        let followers = if followers_on {
                            vec![[400.0, 100.0], [500.0, 400.0]]
                        } else {
                            vec![]
                        };
                        let mut gems = vec![
                            gem(1.0),
                            Gem {
                                life: 0.001,
                                ..gem(2.0)
                            },
                            Gem {
                                x: 400.0,
                                ..gem(3.0)
                            },
                            Gem {
                                lost_one: true,
                                ..gem(4.0)
                            },
                            Gem {
                                x: 400.0,
                                y: 300.0,
                                vx: 100.0,
                                vy: -80.0,
                                ..gem(5.0)
                            },
                            Gem {
                                x: -20.0,
                                y: 1000.0,
                                vx: -10.0,
                                vy: 20.0,
                                ..gem(6.0)
                            },
                            Gem {
                                x: 500.0,
                                y: 400.0,
                                golden: true,
                                ..gem(7.0)
                            },
                            Gem {
                                x: 129.8,
                                ..gem(8.0)
                            },
                            Gem {
                                life: -2.0,
                                x: 700.0,
                                ..gem(9.0)
                            },
                        ];
                        let initial = json!({"p":{"x":p.x,"y":p.y,"magnet":p.magnet},"view":v,"mission":mission,"followers":followers,"dt":dt,"gems":gems});
                        let mut values = Vec::new();
                        update_gems(&mut gems, dt, &p, v, mission == 2, &followers, |g| {
                            values.push(g.value)
                        });
                        cases.push(initial);
                        expected.push(json!({"gems":gems,"collected":values}));
                    }
                }
            }
        }
    }
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find("function updateGems(").unwrap()..];
    let update = &tail[..tail.find("\nfunction ").unwrap()];
    let script = format!(
        r#"
const cases={cases};let P,S,W,groundY,topLimit,gems,mission,me,wingState,current,collected;
const clamp=(x,a,b)=>Math.max(a,Math.min(b,x)),dist=(x,y,a,b)=>Math.hypot(x-a,y-b);
const bonded=()=>({{}}),scaleOf=()=>current.view.dragon_scale;
// collect() begins by excluding lostOne; the remaining reward/progression body is outside this slice.
const collect=g=>{{if(!g.lostOne)collected.push(g.value);}};
{update}
const results=cases.map(c=>{{current=c;P=c.p;S=c.view.scale;W=c.view.width;groundY=c.view.ground_y;topLimit=c.view.top_limit;
 mission=[null,{{done:false}},{{done:true}},{{done:true,failT:1}}][c.mission];
 wingState=new Map(c.followers.map((w,i)=>[i,{{x:w[0],y:w[1]}}]));me={{party:[{{bonded:true}},...c.followers.map((_,i)=>({{id:i,bonded:false}})),{{id:'missing',bonded:false}}]}};
 gems=c.gems.map(g=>{{g.lostOne=g.lost_one;delete g.lost_one;return g;}});collected=[];updateGems(c.dt);
 return {{gems:gems.map(g=>{{g.lost_one=g.lostOne;delete g.lostOne;return g;}}),collected}};
}});print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap()
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
