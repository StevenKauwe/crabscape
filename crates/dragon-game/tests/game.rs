use dragon_game::{
    combat::Shot,
    damage::Hazard,
    flight::{Input, Viewport},
    game::Game,
    gems::Gem,
};
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
#[test]
fn first_flight_can_clear_every_stop_and_sweep_before_results() {
    let mut g = Game::first_flight(view(), 42);
    g.player.safe = 10000.;
    let mut max_stop = 0;
    for _ in 0..6000 {
        // Synthetic perfect shots exercise collision -> defeat -> drops -> waves, not
        // a claim that a stationary human could win or that the game feel is verified.
        for e in &g.enemies {
            if !e.target.dead && !e.target.calm {
                g.shots.push(Shot {
                    x: e.target.x,
                    y: e.target.y,
                    r: 2.,
                    dmg: 10.,
                    life: 1.,
                    owner: "bond".into(),
                    ..Shot::default()
                });
            }
        }
        g.step(0.05, Input::default(), view()).unwrap();
        max_stop = max_stop.max(g.mission.stop);
        if g.result.is_some() {
            break;
        }
    }
    assert_eq!(max_stop, 3);
    assert_eq!(g.result, Some(true));
    assert!(g.kills >= 28);
    assert!(g.xp >= 112.);
    assert!(g.earned > 0.);
    assert_eq!(g.mission.carry, 0.);
    let before = (g.earned, g.xp, g.mission.t, g.rng);
    g.step(
        0.05,
        Input {
            fire: true,
            ..Input::default()
        },
        view(),
    )
    .unwrap();
    assert_eq!(before, (g.earned, g.xp, g.mission.t, g.rng));
}
#[test]
fn last_heart_retires_before_gem_collection_and_results_do_not_bank() {
    let mut g = Game::first_flight(view(), 7);
    g.player.safe = 0.;
    g.player.hearts = 1;
    g.mission.carry = 5.;
    g.hazards.push(Hazard {
        x: g.player.x,
        y: g.player.y,
        r: 20.,
        life: 3.,
        uid: 1,
        ..Hazard::default()
    });
    g.gems.push(Gem {
        x: g.player.x,
        y: g.player.y,
        life: 3.,
        r: 10.,
        value: 2.,
        ..Gem::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert!(g.player.out);
    assert_eq!(g.player.hearts, 0);
    assert_eq!(g.lost, 3.);
    assert_eq!(g.mission.carry, 2.);
    assert_eq!(g.earned, 2.);
    assert_eq!(g.mission.fail_t, 1.6);
    for _ in 0..40 {
        g.step(0.05, Input::default(), view()).unwrap();
    }
    assert_eq!(g.result, Some(false));
    assert_eq!(g.mission.banked, 0.);
}
#[test]
fn seeded_replay_and_invalid_input_are_stable() {
    let mut a = Game::first_flight(view(), 123);
    let mut b = Game::first_flight(view(), 123);
    for i in 0..900 {
        let input = Input {
            fire: true,
            right: i % 70 < 35,
            up: i % 60 < 30,
            ..Input::default()
        };
        a.step(1. / 60., input, view()).unwrap();
        b.step(1. / 60., input, view()).unwrap();
    }
    assert_eq!(a.player, b.player);
    assert_eq!(a.enemies, b.enemies);
    assert_eq!(a.gems, b.gems);
    assert_eq!(a.mission, b.mission);
    assert_eq!(a.rng, b.rng);
    let before = a.player.clone();
    assert!(a.step(f64::NAN, Input::default(), view()).is_err());
    assert_eq!(a.player, before);
}
#[test]
fn zero_time_fires_from_pre_motion_mouth() {
    let mut g = Game::first_flight(view(), 2);
    let x = g.player.x;
    let y = g.player.y;
    g.step(
        0.,
        Input {
            fire: true,
            ..Input::default()
        },
        view(),
    )
    .unwrap();
    assert_eq!(g.shots.len(), 1);
    assert_eq!(g.shots[0].x, x + view().scale * view().dragon_scale * 0.95);
    assert_eq!(g.shots[0].y, y - view().scale * view().dragon_scale * 0.2);
}

#[test]
fn nonlethal_bond_hit_freezes_the_following_frame() {
    let mut g = Game::first_flight(view(), 3);
    g.player.safe = 100.;
    for _ in 0..200 {
        g.step(0.05, Input::default(), view()).unwrap();
        if !g.enemies.is_empty() {
            break;
        }
    }
    let e = &g.enemies[0];
    assert_eq!(e.target.max_hp, 1.15);
    g.shots.push(Shot {
        x: e.target.x,
        y: e.target.y,
        r: 2.,
        dmg: 0.6,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.hit_stop, 0.025);
    assert!(!g.enemies[0].target.dead);
    let t = g.mission.t;
    g.step(0.01, Input::default(), view()).unwrap();
    assert_eq!(g.mission.t, t);
    assert!((g.hit_stop - 0.015).abs() < 1e-12);
}

#[test]
fn late_sweep_gems_are_banked_before_success_result() {
    let mut g = Game::first_flight(view(), 3);
    g.mission.done = true;
    g.mission.end_t = 0.01;
    g.mission.carry = 4.;
    g.gems.push(Gem {
        x: g.player.x,
        y: g.player.y,
        life: 1.,
        r: 10.,
        value: 3.,
        ..Gem::default()
    });
    g.step(0.02, Input::default(), view()).unwrap();
    assert_eq!(g.result, Some(true));
    assert_eq!(g.earned, 7.);
    assert_eq!(g.mission.banked, 7.);
    assert_eq!(g.mission.carry, 0.);
}

fn perfect_shots(g: &mut Game) {
    for e in &g.enemies {
        if !e.target.dead && !e.target.calm {
            g.shots.push(Shot {
                x: e.target.x,
                y: e.target.y,
                r: 2.,
                dmg: 1000.,
                life: 1.,
                owner: "bond".into(),
                ..Shot::default()
            });
        }
    }
}
#[test]
fn all_four_hollowbrook_missions_reach_results() {
    for id in ["m1", "m2", "m3", "m4"] {
        let mut g = Game::start(id, view(), 42).unwrap();
        g.player.safe = 10000.;
        let mut objective_seen = false;
        let mut boss_seen = false;
        for _ in 0..10000 {
            objective_seen |= g.objective.is_some();
            boss_seen |= g.enemies.iter().any(|e| e.target.boss);
            perfect_shots(&mut g);
            g.step(0.05, Input::default(), view()).unwrap();
            if g.result.is_some() {
                break;
            }
        }
        assert_eq!(g.result, Some(true), "{id}");
        assert_eq!(g.mission.stop, 3);
        assert_eq!(objective_seen, id == "m3");
        assert_eq!(boss_seen, id == "m4");
    }
    assert!(Game::start("m5", view(), 1).is_err());
}
#[test]
fn protecting_the_barn_can_fail_the_mission() {
    let mut g = Game::start("m3", view(), 7).unwrap();
    g.player.safe = 10000.;
    for _ in 0..10000 {
        perfect_shots(&mut g);
        g.step(0.05, Input::default(), view()).unwrap();
        if g.objective.is_some() {
            break;
        }
    }
    let ob = g.objective.as_mut().unwrap();
    ob.hp = 1;
    ob.hurt_t = 0.;
    g.hazards.push(Hazard {
        x: ob.x,
        y: ob.y,
        r: 20.,
        life: 3.,
        kind: "spark".into(),
        uid: 99999,
        ..Hazard::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.mission.fail_t, 1.6);
    assert_eq!(g.fail_reason.as_deref(), Some("The hay barn was lost!"));
    for _ in 0..40 {
        g.step(0.05, Input::default(), view()).unwrap();
    }
    assert_eq!(g.result, Some(false));
}
#[test]
fn boss_phase_spawns_help_and_defeat_calms_it_before_adjacent_shot() {
    let mut g = Game::start("m4", view(), 11).unwrap();
    g.player.safe = 10000.;
    for _ in 0..10000 {
        perfect_shots(&mut g);
        g.step(0.05, Input::default(), view()).unwrap();
        if g.enemies.iter().any(|e| e.target.boss) {
            break;
        }
    }
    let b = g.enemies.iter().find(|e| e.target.boss).unwrap();
    let hp = b.target.hp;
    g.shots.push(Shot {
        x: b.target.x,
        y: b.target.y,
        r: 2.,
        dmg: hp * 0.5,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.mission.phase, 1);
    assert_eq!(
        g.enemies
            .iter()
            .filter(|e| e.extra && !e.target.calm)
            .count(),
        4
    );
    let extra = g
        .enemies
        .iter()
        .find(|e| e.extra && !e.target.calm)
        .unwrap();
    let extra_id = extra.target.id;
    let extra_hp = extra.target.hp;
    // Source traverses shots backwards: kill boss first, then the add shot must skip.
    g.shots.push(Shot {
        x: extra.target.x,
        y: extra.target.y,
        r: 2.,
        dmg: 1000.,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    let b = g.enemies.iter().find(|e| e.target.boss).unwrap();
    g.shots.push(Shot {
        x: b.target.x,
        y: b.target.y,
        r: 2.,
        dmg: 1000.,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    let xp = g.xp;
    g.step(0., Input::default(), view()).unwrap();
    let extra = g.enemies.iter().find(|e| e.target.id == extra_id).unwrap();
    assert!(extra.target.calm);
    assert_eq!(extra.target.hp, extra_hp);
    assert!(g.xp >= xp + 80.);
    assert!(g.mission.done);
    assert!(g.gems.iter().filter(|g| g.golden).count() >= 3);
}

fn profile_fixture() -> serde_json::Value {
    use serde_json::json;
    let mut p = dragon_game::profile::new_profile("synthetic");
    p["build"] = json!({"name":"Synthetic","dname":"Dragon","dkind":"bond","path":"forge"});
    p["party"] = json!([{"id":"bond","name":"Dragon","kind":"bond","xp":1960.,"bonded":true},{"id":"wing","name":"Wing","kind":"flame","xp":360.,"bonded":false}]);
    p["craft"] = json!({"tea":2,"tonic":1,"armor":2,"bellows":1,"saddles":true,"treatsMade":0});
    p["skills"] = json!({"breath":5,"rider":4,"magic":4});
    p["charms"] = json!({"heart":1,"pouch":1,"start":1});
    p["cleared"] = json!(["m2", "m3"]);
    p["loadout"] = json!(["thuri", "kaelo"]);
    p
}
#[test]
fn normalized_party_profile_drives_breath_weapon_growth_and_charms() {
    let p = profile_fixture();
    let original = p.clone();
    let mut g = Game::from_profile("m1", p, view(), 1).unwrap();
    assert_eq!(original["charms"]["heart"], 1);
    assert_eq!(g.profile.as_ref().unwrap()["charms"]["heart"], 0.);
    assert_eq!(g.player.hearts, 8);
    assert_eq!(g.max_hearts(), 9);
    assert_eq!(g.player.energy, 130.);
    assert_eq!(g.player.shield, 10.);
    assert_eq!(g.dragon_scale(), 1.);
    g.player.follow_cool.insert("wing".into(), 0.);
    g.step(
        0.,
        Input {
            fire: true,
            ..Input::default()
        },
        view(),
    )
    .unwrap();
    assert_eq!(g.shots.iter().filter(|s| s.owner == "wing").count(), 3);
    assert_eq!(
        g.shots
            .iter()
            .filter(|s| s.kind == dragon_game::combat::Kind::Hammer)
            .count(),
        1
    );
    assert_eq!(
        g.shots
            .iter()
            .filter(|s| s.kind == dragon_game::combat::Kind::Bond)
            .count(),
        3
    );
    assert!((g.player.main_cool - 0.32 * 0.84 * 0.85).abs() < 1e-12);
    assert!((g.player.follow_cool["wing"] - 0.42 * 1.85 * 0.84).abs() < 1e-12);
    assert!(g.wings.contains_key("wing"));
    g.select_main(1).unwrap();
    assert_eq!(g.profile.as_ref().unwrap()["main"], "wing");
    g.swap_main().unwrap();
    assert_eq!(g.profile.as_ref().unwrap()["main"], "bond");
    g.player.shield = 0.;
    g.player.safe = 0.;
    g.player.hearts = 1;
    g.mission.carry = 5.;
    g.hazards.push(Hazard {
        x: g.player.x,
        y: g.player.y,
        r: 20.,
        life: 3.,
        uid: 77,
        ..Hazard::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.lost, 0.);
    assert_eq!(g.mission.carry, 5.);
}
#[test]
fn spells_are_loadout_gated_and_profile_rewards_use_tonic() {
    use dragon_game::abilities::Spell;
    let mut g = Game::from_profile("m1", profile_fixture(), view(), 2).unwrap();
    g.player.shield = 0.;
    g.cast(Spell::Naina);
    assert_eq!(g.player.energy, 130.);
    assert!(g.gems.is_empty());
    g.cast(Spell::Thuri);
    assert_eq!(g.player.energy, 86.);
    assert_eq!(g.player.shield, 6.);
    assert_eq!(g.last_cast.as_deref(), Some("thuri"));
    assert_eq!(g.cast_count, 1);
    let xp = g.xp;
    g.gems.push(Gem {
        x: g.player.x,
        y: g.player.y,
        r: 10.,
        value: 3.,
        life: 1.,
        ..Gem::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.xp, xp + 3.75);
    assert_eq!(g.profile.as_ref().unwrap()["party"][1]["xp"], 363.75);
}
#[test]
fn owner_xp_and_growth_apply_during_collision_not_after_frame() {
    let mut p = profile_fixture();
    p["party"][0]["xp"] = serde_json::json!(357.);
    let mut g = Game::from_profile("m1", p, view(), 11).unwrap();
    g.player.magnet = 0.;
    g.player.safe = 100.;
    for _ in 0..200 {
        g.step(0.05, Input::default(), view()).unwrap();
        if !g.enemies.is_empty() {
            break;
        }
    }
    let e = &g.enemies[0];
    g.shots.push(Shot {
        x: e.target.x,
        y: e.target.y,
        r: 2.,
        dmg: 10.,
        life: 1.,
        owner: "wing".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.xp, 357.);
    assert_eq!(g.profile.as_ref().unwrap()["party"][1]["xp"], 364.);
    g.apply_progression(
        dragon_game::progression::Command::AddXp {
            dragon: "bond".into(),
            amount: 4.,
        },
        0.,
    )
    .unwrap();
    assert_eq!(g.xp, 361.);
    assert_eq!(g.dragon_scale(), 0.8);
    assert_eq!(g.profile.as_ref().unwrap()["stars"], 1.);
}
#[test]
fn daddio_calming_awards_half_drops_and_mega_timer_hits_once() {
    let mut p = profile_fixture();
    p["build"]["name"] = serde_json::json!("Daddio");
    let mut g = Game::from_profile("m1", p, view(), 12).unwrap();
    g.player.safe = 100.;
    assert_eq!(g.dragon_scale(), 1.18);
    for _ in 0..200 {
        g.step(0.05, Input::default(), view()).unwrap();
        if !g.enemies.is_empty() {
            break;
        }
    }
    let n = g.gems.len();
    let xp = g.xp;
    g.dad_power(dragon_game::abilities::DadPower::Said);
    assert!(g.enemies.iter().all(|e| e.target.calm));
    assert_eq!(g.gems.len(), n + 1);
    assert_eq!(g.xp, xp + 4.);
    assert_eq!(g.last_cast.as_deref(), Some("said"));
    g.special();
    assert_eq!(g.player.mega_t, 0.6);
    for _ in 0..15 {
        g.step(0.05, Input::default(), view()).unwrap();
    }
    assert!(g.player.mega_t <= 0.);
}

#[test]
fn profile_completion_unlocks_once_after_last_gem_bank() {
    let mut p = profile_fixture();
    p["cleared"] = serde_json::json!([]);
    let mut g = Game::from_profile("m2", p, view(), 3).unwrap();
    g.completion_day = 12345;
    g.completion_now_ms = 12345678;
    g.mission.done = true;
    g.mission.end_t = 0.01;
    g.mission.carry = 4.;
    g.step(0.02, Input::default(), view()).unwrap();
    let p = g.profile.as_ref().unwrap();
    assert!(
        p["cleared"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("m2"))
    );
    assert_eq!(p["eggs"], 1.);
    assert_eq!(g.mission.banked, 4.);
    assert_eq!(p["earned"], g.earned);
    assert!(
        p["loadout"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("thuri"))
    );
    let before = p.clone();
    g.step(0.05, Input::default(), view()).unwrap();
    assert_eq!(g.profile.as_ref().unwrap(), &before);
}
fn source_block(start: &str, end: &str) -> &'static str {
    let source = include_str!("../../../web/dragon/index.html");
    let tail = &source[source.find(start).unwrap()..];
    let finish = tail.find(end).unwrap();
    &tail[..finish
        + if end == "\n};" || end == "\n];" {
            end.len()
        } else {
            0
        }]
}
fn compare_json(a: &serde_json::Value, b: &serde_json::Value) {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            assert!((a - b).abs() < 1e-9 * b.abs().max(1.), "{a} != {b}");
        }
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in a {
                compare_json(v, &b[k]);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare_json(a, b);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_profile_shooting_and_wing_initialization_match() {
    use serde_json::json;
    let mut cases = vec![];
    let mut expected = vec![];
    for (ki, kind) in ["bond", "flame", "spark", "boulder", "frost", "star"]
        .into_iter()
        .enumerate()
    {
        for (si, xp) in [0., 360., 1960., 4840.].into_iter().enumerate() {
            for main in ["bond", "wing"] {
                let mut p = profile_fixture();
                p["party"][0]["xp"] = json!(xp);
                p["party"][0]["kind"] = json!(kind);
                p["main"] = json!(main);
                p["build"]["path"] = json!(["shadow", "forge", "storm", "guardian"][si]);
                let seed = (ki * 20 + si + 1) as u32;
                let mut g = Game::from_profile("m1", p.clone(), view(), seed).unwrap();
                g.player.x = 200.;
                g.player.y = 300.;
                g.player.vx = 31.;
                g.player.vy = -17.;
                g.player.facing = -1;
                g.player.follow_cool.insert("bond".into(), 0.);
                g.player.follow_cool.insert("wing".into(), 0.);
                g.step(
                    0.,
                    Input {
                        fire: true,
                        ..Input::default()
                    },
                    view(),
                )
                .unwrap();
                cases.push(json!({"profile":p,"seed":seed}));
                expected.push(json!({"shots":g.shots,"mainCool":g.player.main_cool,"followCool":g.player.follow_cool,"weaponCool":g.player.weapon_cool,"fireFlash":g.player.fire_flash,"wings":g.wings.iter().map(|(k,w)|(k.clone(),json!([w.x,w.y,w.t]))).collect::<serde_json::Map<String,serde_json::Value>>()}));
            }
        }
    }
    let functions = [
        "mouthOf",
        "breathCd",
        "breath",
        "riderWeapon",
        "shootAll",
        "updateWing",
        "mulberry",
    ]
    .map(|n| source_block(&format!("function {n}("), "\nfunction "))
    .join("\n");
    let script = format!(
        r#"
const cases={cases},defaults={defaults};
{kinds}
{stages}
{paths}
let me,P,R,shots,wingState,time=0;const S={scale};
const bonded=()=>me.party[0],mainDragon=()=>me.party.find(d=>d.id===me.main)||me.party[0],wingDragon=()=>{{const m=mainDragon();return !m.bonded?me.party[0]:me.party.find(d=>d!==m)||null;}};
const levelOf=xp=>Math.min(20,Math.floor(1+Math.sqrt(Math.max(0,xp)/40))),stageOf=d=>levelOf(d.xp)>=12?3:levelOf(d.xp)>=8?2:levelOf(d.xp)>=4?1:0,scaleOf=d=>STAGES[stageOf(d)].scale;
const skillLv=k=>me.skills[k]||0,teaLv=()=>me.craft.tea,bellowsMult=()=>1+.2*me.craft.bellows,isDaddio=()=>false,riding=()=>levelOf(bonded().xp)>=8&&me.craft.saddles;
const rand=(a,b)=>a+Math.random()*(b-a),SHOT_COLORS={{}},sfx={{breath:()=>{{}},weapon:()=>{{}}}};
{functions}
const results=cases.map(c=>{{me=c.profile;R={{path:me.build.path,breath:[]}};P={{x:200,y:300,vx:31,vy:-17,facing:-1,mainCool:0,followCool:{{bond:0,wing:0}},weaponCool:0,fireFlash:0}};shots=[];wingState=new Map();Math.random=mulberry(c.seed);shootAll();updateWing(0);return {{shots:shots.map(s=>{{const out={{...defaults,...s}};delete out.cols;out.hits=Array.from(out.hits);return out;}}),mainCool:P.mainCool,followCool:P.followCool,weaponCool:P.weaponCool,fireFlash:P.fireFlash,wings:Object.fromEntries([...wingState].map(([id,w])=>[id,[w.x,w.y,w.t]]))}};}});print(JSON.stringify(results));
"#,
        cases = serde_json::to_string(&cases).unwrap(),
        defaults = serde_json::to_string(&Shot::default()).unwrap(),
        kinds = source_block("const KINDS = {", "\n};"),
        stages = source_block("const STAGES = [", "\n];"),
        paths = source_block("const PATHS = {", "\n};"),
        scale = view().scale
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
    let actual: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    compare_json(&json!(expected), &actual);
}
