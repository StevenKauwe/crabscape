use dragon_game::combat::{Kind, Shot};
use dragon_game::projectiles::{Authority, Event, ShotWorld, Target, hit_enemy, update_shots};
use serde_json::{Value, json};
fn world() -> ShotWorld {
    ShotWorld {
        width: 800.0,
        height: 600.0,
        scale: 40.0,
        player: [100.0, 100.0],
        authority: Authority::default(),
    }
}
fn target(id: u64) -> Target {
    Target {
        id,
        x: 100.0,
        y: 100.0,
        r: 20.0,
        hp: 10.0,
        max_hp: 10.0,
        ..Target::default()
    }
}
fn shot(kind: Kind) -> Shot {
    Shot {
        kind,
        x: 100.0,
        y: 100.0,
        life: 1.0,
        max: 1.0,
        r: 4.0,
        dmg: 1.0,
        ..Shot::default()
    }
}
#[test]
fn mark_and_crack_apply_after_damage_and_mega_bypasses_both() {
    let mut e = Target {
        boss: true,
        armor: 0.25,
        ..target(1)
    };
    let mut events = Vec::new();
    let s = Shot {
        mark: true,
        ..shot(Kind::Hammer)
    };
    hit_enemy(&mut e, &s, Authority::default(), &mut events);
    assert_eq!((e.hp, e.marked, e.cracked), (9.0, 3.0, 6.0));
    hit_enemy(&mut e, &s, Authority::default(), &mut events);
    assert!((e.hp - 6.9).abs() < 1e-12);
    hit_enemy(
        &mut e,
        &Shot { mega: true, ..s },
        Authority::default(),
        &mut events,
    );
    assert!((e.hp - 2.9).abs() < 1e-12);
}
#[test]
fn guest_only_flashes_and_slows_and_requests_host_hit() {
    let mut e = target(1);
    let mut events = Vec::new();
    hit_enemy(
        &mut e,
        &Shot {
            mega: true,
            mark: true,
            crack: true,
            slow: 2.5,
            stun: 3.0,
            ..shot(Kind::Hammer)
        },
        Authority {
            guest: true,
            ..Authority::default()
        },
        &mut events,
    );
    assert_eq!(
        (e.hp, e.flash, e.slow, e.stun, e.marked, e.cracked),
        (10.0, 0.12, 2.5, 0.0, 0.0, 0.0)
    );
    assert!(matches!(events.as_slice(), [Event::GuestHit { id: 1, .. }]));
}
#[test]
fn arrow_hits_two_in_enemy_order_and_piercing_remembers_ids() {
    let mut enemies = vec![target(3), target(2), target(1)];
    let mut shots = vec![Shot {
        pierce: true,
        ..shot(Kind::Arrow)
    }];
    update_shots(&mut shots, &mut enemies, 0.01, world());
    assert!(shots.is_empty());
    assert_eq!(
        enemies.iter().map(|e| e.hp).collect::<Vec<_>>(),
        [9.0, 9.0, 10.0]
    );
    let mut shots = vec![Shot {
        pierce: true,
        ..shot(Kind::Boulder)
    }];
    update_shots(&mut shots, &mut enemies, 0.01, world());
    enemies.swap(0, 2);
    update_shots(&mut shots, &mut enemies, 0.01, world());
    assert_eq!(
        enemies.iter().map(|e| e.hp).collect::<Vec<_>>(),
        [9.0, 8.0, 8.0]
    );
}
#[test]
fn reverse_shot_order_and_collision_before_expiry_or_bounds() {
    let mut enemies = vec![Target {
        hp: 1.0,
        x: -81.0,
        ..target(1)
    }];
    let mut shots = vec![
        Shot {
            x: -81.0,
            life: 0.0,
            owner: "first".into(),
            ..shot(Kind::Bond)
        },
        Shot {
            x: -81.0,
            life: 0.0,
            owner: "last".into(),
            dad: true,
            ..shot(Kind::Bond)
        },
    ];
    let events = update_shots(&mut shots, &mut enemies, 0.01, world());
    assert_eq!(
        events,
        vec![
            Event::Defeat {
                id: 1,
                owner: "last".into()
            },
            Event::DadImpact { x: -81.0, y: 100.0 }
        ]
    );
    assert!(shots.is_empty());
}
#[test]
fn hammer_catch_precedes_collisions_and_boss_sibling_damage_can_defeat() {
    let mut enemies = vec![target(1)];
    let mut shots = vec![Shot {
        age: 0.45,
        ..shot(Kind::Hammer)
    }];
    update_shots(&mut shots, &mut enemies, 0.01, world());
    assert!(shots.is_empty());
    assert_eq!(enemies[0].hp, 10.0);
    enemies[0].boss = true;
    let mut events = Vec::new();
    hit_enemy(
        &mut enemies[0],
        &shot(Kind::Bond),
        Authority {
            sibling_boss_damage: 9.0,
            guest: false,
        },
        &mut events,
    );
    assert!(enemies[0].dead);
    assert_eq!(events.len(), 1);
}
fn source(name: &str) -> &str {
    let html = include_str!("../../../web/dragon/index.html");
    let rest = &html[html.find(&format!("function {name}(")).unwrap()..];
    &rest[..rest.find("\nfunction ").unwrap()]
}
fn close(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() <= 1e-9 * b.abs().max(1.0), "{a} != {b}");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                close(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            for (key, a) in a {
                close(a, &b[key]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn exact_legacy_source_matches_projectile_and_damage_fixtures() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for kind in [
        Kind::Bond,
        Kind::Flame,
        Kind::Spark,
        Kind::Boulder,
        Kind::Frost,
        Kind::Star,
        Kind::Arrow,
        Kind::Hammer,
        Kind::Area,
    ] {
        for variant in 0..12 {
            let mut w = world();
            w.authority.guest = variant == 1;
            w.authority.sibling_boss_damage = 1.0;
            let mut enemies = vec![
                Target {
                    boss: variant % 2 == 0,
                    armor: 0.3,
                    marked: if variant == 2 { 2.0 } else { 0.0 },
                    cracked: if variant == 3 { 2.0 } else { 0.0 },
                    ..target(2)
                },
                target(1),
                target(3),
            ];
            if variant == 4 {
                enemies[0].calm = true;
            }
            let mut shots = vec![Shot {
                vx: if variant == 5 { -70.0 } else { 25.0 },
                vy: 0.1,
                pierce: matches!(kind, Kind::Arrow | Kind::Hammer | Kind::Boulder),
                homing: kind == Kind::Star,
                age: if variant == 6 { 0.46 } else { 0.0 },
                slow: 2.5,
                stun: 0.3,
                mark: true,
                crack: variant == 7,
                mega: variant == 8,
                life: if variant == 9 { 0.001 } else { 1.0 },
                dad: true,
                ..shot(kind)
            }];
            if variant == 10 {
                shots[0].x = 150.0;
                shots[0].y = 101.0;
            }
            if variant == 11 {
                shots.push(Shot {
                    dmg: 12.0,
                    ..shot(Kind::Bond)
                });
            }
            let frames = [0.01, 0.04, 0.1, 0.4];
            cases.push(
                json!({"shots":shots,"enemies":enemies,"guest":w.authority.guest,"frames":frames}),
            );
            let mut states = Vec::new();
            for dt in frames {
                let events = update_shots(&mut shots, &mut enemies, dt, w);
                states.push(json!({"shots":shots,"enemies":enemies,"events":events}));
            }
            expected.push(json!(states));
        }
    }
    let script = format!(
        r#"
const cases={cases}, TAU=Math.PI*2, S=40, W=800,H=600,P={{x:100,y:100}};
const dist=(a,b,c,d)=>Math.hypot(a-c,b-d),clamp=(v,a,b)=>Math.max(a,Math.min(b,v));
let shots,enemies,events,mission,hitStop=0; const texts=[],particles=[],SHOT_COLORS={{flame:['a','b','c']}},sfx={{smash(){{}},thud(){{}}}},burst=()=>{{}},sibBossDmg=()=>1;
const ri=0,remoteBy=-1;const room={{emit(_,d){{events.push({{GuestHit:{{id:d.uid,dmg:d.dmg,kind:d.kind,slow:d.slow,stun:d.stun,mega:!!d.mega,mark:!!d.mk,crack:!!d.cr}}}});return Promise.resolve();}}}};
const defeat=(e,owner)=>{{e.dead=true;events.push({{Defeat:{{id:e.id,owner}}}});}},dadImpact=(x,y)=>events.push({{DadImpact:{{x,y}}}});
Math.random=()=>0.99;
{hit}
{update}
const results=cases.map(c=>{{shots=c.shots;enemies=c.enemies;mission={{guest:c.guest,def:{{id:1}}}};
for(const e of enemies){{e.uid=e.id;e.maxHp=e.max_hp;e.def={{armor:e.armor}};}}
for(const s of shots)s.hits=new Set(s.hits.map(id=>enemies.find(e=>e.id===id)));
return c.frames.map(dt=>{{events=[];updateShots(dt);return {{shots:shots.map(s=>({{...s,hits:[...s.hits].map(e=>e.id).sort((a,b)=>a-b)}})),enemies:enemies.map(e=>({{...e}})),events:events.slice()}};}});}});print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        hit = source("hitEnemy"),
        update = source("updateShots")
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
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let actual: Value = serde_json::from_slice(&out.stdout).unwrap();
    close(&json!(expected), &actual);
}

#[test]
fn homing_wraps_at_pi_keeps_speed_and_uses_first_nearest_target() {
    let mut enemies = vec![
        Target {
            x: 0.0,
            y: 99.0,
            r: 0.0,
            ..target(1)
        },
        Target {
            x: 200.0,
            y: 101.0,
            r: 0.0,
            ..target(2)
        },
    ];
    let mut shots = vec![Shot {
        vx: -10.0,
        vy: 0.01,
        homing: true,
        r: 0.0,
        ..shot(Kind::Star)
    }];
    update_shots(&mut shots, &mut enemies, 0.01, world());
    assert!(shots[0].vx < 0.0 && shots[0].vy < 0.0);
    assert!((shots[0].vx.hypot(shots[0].vy) - 10.0_f64.hypot(0.01)).abs() < 1e-12);
}

#[test]
fn strict_collision_and_world_boundaries_and_hammer_turn_threshold() {
    let mut enemies = vec![Target {
        x: 122.0,
        ..target(1)
    }];
    let mut shots = vec![shot(Kind::Bond)];
    update_shots(&mut shots, &mut enemies, 0.0, world());
    assert_eq!(enemies[0].hp, 10.0); // 0.9 * 20 + 4 is excluded.
    shots = vec![Shot {
        x: -80.0,
        y: -80.0,
        ..shot(Kind::Bond)
    }];
    update_shots(&mut shots, &mut [], 0.0, world());
    assert_eq!(shots.len(), 1);
    shots[0].x -= 0.001;
    update_shots(&mut shots, &mut [], 0.0, world());
    assert!(shots.is_empty());
    shots = vec![Shot {
        x: 200.0,
        age: 0.45,
        vx: 9.0,
        ..shot(Kind::Hammer)
    }];
    update_shots(&mut shots, &mut [], 0.0, world());
    assert_eq!(shots[0].vx, 9.0);
    update_shots(&mut shots, &mut [], 0.1, world());
    assert!((shots[0].vx + 215.5).abs() < 1e-12);
}
