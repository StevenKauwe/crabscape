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
    assert!(Game::start("m25", view(), 1).is_err());
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

#[test]
fn every_base_mission_finishes_with_its_authored_objective() {
    let sea = Game::from_profile("m13", profile_fixture(), view(), 1).unwrap();
    assert_eq!(sea.profile.as_ref().unwrap()["lastRealm"], "sea");
    for n in 1..=24 {
        let id = format!("m{n}");
        let mut g = Game::start(&id, view(), n * 17).unwrap();
        g.player.safe = 10000.;
        let mut seen_objective = false;
        for _ in 0..10000 {
            if let Some(o) = &g.objective {
                seen_objective = true;
                if o.kind == dragon_game::objectives::Kind::Carry && !o.done {
                    g.player.x = if o.carrier < 0 { o.x } else { o.to };
                    g.player.y = o.y;
                }
            }
            perfect_shots(&mut g);
            g.step(0.05, Input::default(), view()).unwrap();
            if g.result.is_some() {
                break;
            }
        }
        assert_eq!(
            g.result,
            Some(true),
            "{id} at stop{} with queue{:?}, kills{}/{}",
            g.mission.stop,
            g.mission.queue,
            g.mission.kills,
            g.mission.total
        );
        assert_eq!(g.mission.stop, 3, "{id}");
        assert_eq!(
            seen_objective,
            matches!(n, 3 | 6 | 11 | 14 | 15 | 17 | 18 | 21)
        );
    }
    assert!(Game::start("m25c", view(), 1).is_err());
}
fn fixture_enemy(g: &mut Game, kind: dragon_game::enemies::Kind, at: [f64; 2]) -> usize {
    use dragon_game::enemies::{Enemy, World};
    let v = view();
    let world = World {
        width: v.width,
        ground_y: v.ground_y,
        top: v.top_limit,
        sky_bottom: v.ground_y
            - v.scale
                * if matches!(g.definition.realm, 1 | 2 | 4) {
                    2.6
                } else {
                    0.5
                },
        scale: v.scale,
        player: [g.player.x, g.player.y],
        aim: [g.player.x, g.player.y],
        quiet: false,
        safe: true,
        dragon_scale: g.dragon_scale(),
    };
    let scaling = g.scaling();
    let i = g.enemies.len();
    let mut e = Enemy::spawn(
        kind,
        500 + i as u64,
        false,
        Some(at),
        &world,
        scaling,
        &mut || g.rng.next_unit(),
    );
    e.target.x = at[0];
    e.target.y = at[1];
    e.entered = true;
    e.cool = 100.;
    e.cool2 = 100.;
    e.strike_cd = Some(100.);
    g.enemies.push(e);
    i
}
#[test]
fn piercing_slime_death_appends_children_before_next_collision() {
    use dragon_game::enemies::Kind;
    let mut g = Game::start("m17", view(), 5).unwrap();
    g.player.safe = 100.;
    g.mission.leg = dragon_game::missions::Leg::Fight;
    g.mission.wave = 0;
    g.mission.total = 1;
    fixture_enemy(
        &mut g,
        Kind::Slime,
        [500., view().ground_y - view().scale * 0.62 * 0.95],
    );
    g.shots.push(Shot {
        x: 500.,
        y: view().ground_y - 20.,
        r: 100.,
        dmg: 1000.,
        life: 1.,
        pierce: true,
        owner: "bond".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.kills, 3);
    assert_eq!(g.xp, 12.);
    assert!(g.enemies.iter().all(|e| e.target.calm || e.target.dead));
    assert_eq!(g.mission.leg, dragon_game::missions::Leg::Fly);
}
#[test]
fn sea_boss_extra_cap_is_refreshed_and_hazards_are_live() {
    use dragon_game::enemies::{HazardKind, Kind};
    let _ = HazardKind::Fall;
    let mut g = Game::start("m16", view(), 6).unwrap();
    g.player.safe = 100.;
    let i = fixture_enemy(&mut g, Kind::Serpent, [500., 400.]);
    g.enemies[i].windup = 0.01;
    g.step(0.02, Input::default(), view()).unwrap();
    assert_eq!(g.hazards.iter().filter(|h| h.kind == "fall").count(), 5);
    assert_eq!(
        g.enemies
            .iter()
            .filter(|e| e.kind == Kind::Leaper && e.extra)
            .count(),
        2
    );
    let i = g
        .enemies
        .iter()
        .position(|e| e.kind == Kind::Serpent)
        .unwrap();
    g.enemies[i].windup = 0.01;
    g.step(0.02, Input::default(), view()).unwrap();
    assert_eq!(
        g.enemies
            .iter()
            .filter(|e| e.kind == Kind::Leaper && e.extra)
            .count(),
        4
    );
    let i = g
        .enemies
        .iter()
        .position(|e| e.kind == Kind::Serpent)
        .unwrap();
    g.enemies[i].windup = 0.01;
    g.step(0.02, Input::default(), view()).unwrap();
    assert_eq!(
        g.enemies
            .iter()
            .filter(|e| e.kind == Kind::Leaper && e.extra)
            .count(),
        4
    );
}
#[test]
fn gobblers_consume_a_fresh_gem_snapshot_each_enemy_and_stings_cost_one_heart() {
    use dragon_game::enemies::Kind;
    let mut g = Game::start("m21", view(), 7).unwrap();
    g.player.safe = 100.;
    fixture_enemy(&mut g, Kind::Gobbler, [500., 300.]);
    fixture_enemy(&mut g, Kind::Gobbler, [500., 300.]);
    for value in [1., 3.] {
        g.gems.push(Gem {
            x: 500.,
            y: 300.,
            r: 10.,
            life: 3.,
            value,
            ..Gem::default()
        });
    }
    g.step(0., Input::default(), view()).unwrap();
    assert!(g.gems.is_empty());
    assert_eq!(g.mission.carry, 0.);
    assert_eq!(g.xp, 0.);
    let mut g = Game::start("m13", view(), 9).unwrap();
    g.player.x = 300.;
    g.player.y = 300.;
    g.player.safe = 0.;
    let i = fixture_enemy(&mut g, Kind::Jelly, [300. + view().scale * 1.2, 300.]);
    g.enemies[i].risen = true;
    g.enemies[i].sting = 0.4;
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.player.hearts, 4);
    assert_eq!(g.bonks, 1);
}
#[test]
fn carrying_piece_drops_before_retirement_and_a_boss_inhale_ignores_shield() {
    use dragon_game::{
        enemies::Kind,
        objectives::{Objective, specification},
    };
    let mut g = Game::start("m17", view(), 9).unwrap();
    let mut ob = Objective::new(&specification("m17", 3).unwrap(), view());
    ob.carrier = 0;
    g.objective = Some(ob);
    g.player.hearts = 1;
    g.player.safe = 0.;
    g.hazards.push(Hazard {
        x: g.player.x,
        y: g.player.y,
        r: 20.,
        life: 2.,
        uid: 500,
        ..Hazard::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    let ob = g.objective.as_ref().unwrap();
    assert_eq!(ob.carrier, -1);
    assert_eq!(ob.steal_cd, 1.);
    assert!(g.player.out);
    let mut g = Game::start("m24", view(), 10).unwrap();
    g.player.x = 150.;
    g.player.y = 300.;
    g.player.shield = 10.;
    let i = fixture_enemy(&mut g, Kind::Swallower, [500., 150.]);
    g.enemies[i].windup = 2.;
    let p = [g.player.x, g.player.y];
    g.step(0.05, Input::default(), view()).unwrap();
    assert!(g.player.x > p[0]);
    assert!(g.player.y < p[1]);
    assert_eq!(g.player.hearts, 5);
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn actual_source_realm_scaling_split_children_falling_rocks_and_inhale_match() {
    use dragon_game::enemies::Kind;
    use serde_json::json;
    let scaling: Vec<_> = (1..=24)
        .map(|n| {
            let g = Game::start(&format!("m{n}"), view(), 1).unwrap();
            let s = g.scaling();
            json!([s.difficulty, s.speed, s.toughness])
        })
        .collect();
    let mut split = Game::start("m17", view(), 22).unwrap();
    split.player.safe = 100.;
    split.mission.leg = dragon_game::missions::Leg::Fight;
    split.mission.wave = 0;
    split.mission.total = 1;
    fixture_enemy(
        &mut split,
        Kind::Slime,
        [500., view().ground_y - view().scale * 0.62 * 0.95],
    );
    let parent = split.enemies[0].clone();
    let seed = split.rng.state;
    let sc = split.scaling();
    split.shots.push(Shot {
        x: parent.target.x,
        y: parent.target.y,
        r: 2.,
        dmg: 1000.,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    split.step(0., Input::default(), view()).unwrap();
    let parent_json = json!({"type":"slime","x":parent.target.x,"y":parent.target.y,"r":parent.target.r,"extra":false,"dead":false,"calm":false,"boss":false});
    let children:Vec<_>=split.enemies.iter().map(|e|json!({"type":e.kind,"x":e.target.x,"y":e.target.y,"r":e.target.r,"hp":e.target.hp,"maxHp":e.target.max_hp,"t":e.t,"wob":e.wob,"spd":e.speed,"cool":e.cool,"cool2":e.cool2,"extra":e.extra,"entered":e.entered,"dir":e.dir,"air":e.air,"vx":e.velocity[0],"vy":e.velocity[1],"dead":e.target.dead,"calm":e.target.calm})).collect();
    let mut rocks = Game::start("m12", view(), 23).unwrap();
    rocks.player.safe = 100.;
    let i = fixture_enemy(
        &mut rocks,
        Kind::Cavetroll,
        [500., view().ground_y - view().scale * 1.75 * 0.95],
    );
    rocks.enemies[i].cool2 = 0.;
    let rock_seed = rocks.rng.state;
    let rock_parent = json!({"type":"cavetroll","x":rocks.enemies[i].target.x,"y":rocks.enemies[i].target.y,"r":rocks.enemies[i].target.r,"boss":true});
    let rock_player = json!({"x":rocks.player.x,"y":rocks.player.y,"vx":0,"out":false});
    rocks.step(0., Input::default(), view()).unwrap();
    let hazards:Vec<_>=rocks.hazards.iter().map(|h|json!({"uid":h.uid,"kind":h.kind,"x":h.x,"y":h.y,"vx":h.vx,"vy":h.vy,"g":h.gravity,"r":h.r,"warn":h.warn,"life":h.life,"spin":h.spin})).collect();
    let mut inhale = Game::start("m24", view(), 24).unwrap();
    inhale.player.x = 150.;
    inhale.player.y = 300.;
    inhale.player.shield = 10.;
    let i = fixture_enemy(&mut inhale, Kind::Swallower, [500., 150.]);
    inhale.enemies[i].windup = 2.;
    let inhale_player = json!({"x":inhale.player.x,"y":inhale.player.y,"down":0});
    inhale.step(0.05, Input::default(), view()).unwrap();
    let e = &inhale.enemies[0];
    let inhale_enemy = json!({"type":"swallower","x":e.target.x,"y":e.target.y,"r":e.target.r,"dir":e.dir,"windup":e.windup,"calm":false});
    let source = include_str!("../../../web/dragon/index.html");
    let team_tough = source
        .lines()
        .find(|l| l.starts_with("function teamToughness("))
        .unwrap();
    let tough = source
        .lines()
        .find(|l| l.starts_with("const REALM_TOUGH ="))
        .unwrap();
    let funcs = [
        "mulberry",
        "makeEnemy",
        "spawnEnemy",
        "spawnBaby",
        "defeat",
        "bossSpecial",
        "retarget",
        "aimFor",
        "inhalePull",
        "clampPlayer",
    ]
    .map(|n| source_block(&format!("function {n}("), "\nfunction "))
    .join("\n");
    let script = format!(
        r#"
{mission_defs}
{enemy_defs}
{realms}
{tough}
{power}
{team_tough}
const W={w},H={h},S={s},groundY={gy},topLimit={top},BOSS_HP=1.35,reduceMotion=true;
let mission=null,realmIdx=4,enemies=[],hazards=[],strikes=[],gems=[],particles=[],texts=[],shake=0,uidN=0,P={{}},mStats={{kills:0}},xp=0;const ghosts=new Map(),me={{party:[{{id:'bond'}}]}},room=null;
const nextUid=()=>++uidN,rand=(a,b)=>a+Math.random()*(b-a),pick=a=>a[Math.floor(Math.random()*a.length)],clamp=(v,a,b)=>Math.max(a,Math.min(b,v)),skyToughness=()=>1,skyBottom=()=>groundY-S*([1,2,4].includes(realmIdx)?2.6:.5),scaleOf=()=>.62,bonded=()=>me.party[0],defeatFx=()=>{{}},addXP=(d,n)=>xp+=n,othersInMission=()=>false,sfx={{smash:()=>{{}}}};
{funcs}
const scaling=MISSIONS.map(def=>{{const p=missionPower(def);return [p.diff,p.spd,teamToughness(def)];}});
mission={{def:MISSIONS[16],diff:{diff},tough:{toughness},spd:{speed},total:1,kills:0,guest:false}};Math.random=mulberry({seed});let parent={parent};parent.def=ENEMIES.slime;enemies=[parent];defeat(parent,'bond');const fields=['type','x','y','r','hp','maxHp','t','wob','spd','cool','cool2','extra','entered','dir','air','vx','vy','dead','calm'];const split={{children:enemies.filter(e=>!e.dead).map(e=>Object.fromEntries(fields.map(k=>[k,e[k]]))),total:mission.total,kills:mission.kills,xp}};
uidN=0;hazards=[];mission={{def:MISSIONS[11]}};P={rock_player};let rock={rock_parent};enemies=[rock];Math.random=mulberry({rock_seed});retarget(rock);bossSpecial(rock);const rocks=hazards;
P={inhale_player};enemies=[{inhale_enemy}];Math.random=()=>.5;inhalePull(.05);print(JSON.stringify({{scaling,split,rocks,inhale:[P.x,P.y]}}));
"#,
        mission_defs = source_block("const MISSIONS = [", "\n];"),
        enemy_defs = source_block("const ENEMIES = {", "\n};"),
        realms = source_block("const REALMS = [", "\n];"),
        power = source_block("function missionPower(", "\nconst CHAMPS"),
        w = view().width,
        h = view().height,
        s = view().scale,
        gy = view().ground_y,
        top = view().top_limit,
        diff = sc.difficulty,
        toughness = sc.toughness,
        speed = sc.speed,
        parent = parent_json,
        rock_player = rock_player,
        rock_parent = rock_parent,
        inhale_player = inhale_player,
        inhale_enemy = inhale_enemy
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
    compare_json(
        &json!({"scaling":scaling,"split":{"children":children,"total":split.mission.total,"kills":split.mission.kills,"xp":split.xp},"rocks":hazards,"inhale":[inhale.player.x,inhale.player.y]}),
        &actual,
    );
}

#[test]
fn every_champion_finishes_and_rewards_the_base_crown() {
    for n in 1..=24 {
        let mut g = Game::from_profile(&format!("m{n}c"), profile_fixture(), view(), n).unwrap();
        assert!(g.champion && g.par > 0.);
        g.player.safe = 10000.;
        for _ in 0..14000 {
            if let Some(o) = &g.objective
                && o.kind == dragon_game::objectives::Kind::Carry
                && !o.done
            {
                g.player.x = if o.carrier < 0 { o.x } else { o.to };
                g.player.y = o.y;
            }
            perfect_shots(&mut g);
            g.step(0.05, Input::default(), view()).unwrap();
            if g.result.is_some() {
                break;
            }
        }
        assert_eq!(g.result, Some(true), "m{n}c");
        assert_eq!(g.mission.stop, if n % 4 == 0 { 3 } else { 4 });
        assert!(
            g.profile.as_ref().unwrap()["crowns"][format!("m{n}")]
                .as_u64()
                .unwrap_or(0)
                > 0
        );
    }
}

#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn actual_source_champions_and_wild_ally_spawn_match() {
    use serde_json::json;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let champions: Vec<_> = (1..=24).map(|n| {
        let g = Game::start(&format!("m{n}c"), view(), 1).unwrap();
        json!({"id":g.definition.id,"waves":g.definition.waves,"par":g.par,"diff":g.scaling().difficulty,"speed":g.scaling().speed,"tough":g.scaling().toughness})
    }).collect();
    let mut p = profile_fixture();
    p["party"].as_array_mut().unwrap().truncate(1);
    p["hatched"] = json!(20);
    p["wild"] = json!([{"name":"Fern","color":"#123456"}]);
    let mut g = Game::from_profile("m1", p, view(), 123).unwrap();
    g.ally_t = 0.;
    g.step(0., Input::default(), view()).unwrap();
    let script = format!(
        r#"
{defs}
{enemy}
const LEG_FIRST=8,LEG_TIME=15;
{stretch}
{champ}
{power}
const REALM_TOUGH=[1.15,1.5,2.3,4,5,6];
{tough}
const out=MISSIONS.map(m=>{{const c=makeChamp(m),p=missionPower(c),t=teamToughness(c);return {{id:c.id,waves:c.waves,par:Math.ceil((c.travel+(c.par-c.travel)*(c.boss?1.5:1.15)*(0.5+0.5*t/3))/5)*5,diff:p.diff,speed:p.spd,tough:t}};}});
let seed=123; Math.random=()=>{{seed=(seed+0x6D2B79F5)>>>0;let t=seed;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return ((t^t>>>14)>>>0)/4294967296;}};
const S={s},W={w},topLimit={top},allies=[],allWild=()=>[{{name:'Fern',color:'#123456'}}],pick=a=>a[Math.floor(Math.random()*a.length)],rand=(a,b)=>a+Math.random()*(b-a),skyBottom=()=>{bottom},toast=()=>{{}};
{spawn}
spawnAlly(); let allyT=14; const enemies=[],msReached=()=>true; {update_allies} updateAllies(0.05); print(JSON.stringify([out,allies]));
"#,
        defs = source_block("const MISSIONS = [", "\n];"),
        enemy = source_block("const ENEMIES = {", "\n};"),
        stretch = source_block("function stretchMission(", "\nfunction makeChamp"),
        champ = source_block("function makeChamp(", "\n// How much tougher"),
        power = source_block("function missionPower(", "\nconst CHAMPS"),
        tough = source_block("function teamToughness(", "\n// Every other"),
        spawn = source_block("function spawnAlly(", "\nfunction updateAllies"),
        update_allies = source_block("function updateAllies(", "\n/* ---------- Shared sky"),
        s = view().scale,
        w = view().width,
        top = view().top_limit,
        bottom = view().ground_y - view().scale * 0.5
    );
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
    g.step(0.05, Input::default(), view()).unwrap();
    compare_json(
        &json!([champions, g.allies]),
        &serde_json::from_slice(&out.stdout).unwrap(),
    );
}

#[test]
fn wild_allies_require_milestones_and_fire_after_projectiles() {
    use dragon_game::enemies::Kind;
    use serde_json::json;
    for (hatched, boss, expected) in [
        (7, false, 0),
        (7, true, 0),
        (8, false, 0),
        (8, true, 2),
        (19, false, 0),
        (20, false, 1),
        (20, true, 2),
    ] {
        let mut p = profile_fixture();
        p["hatched"] = json!(hatched);
        p["craft"]["bellows"] = json!(2);
        let mut g = Game::from_profile("m1", p, view(), 17).unwrap();
        g.ally_t = 0.;
        fixture_enemy(
            &mut g,
            if boss { Kind::Thunderhead } else { Kind::Cloud },
            [800., 200.],
        );
        g.player.safe = 100.;
        g.step(0., Input::default(), view()).unwrap();
        assert_eq!(g.allies.len(), expected, "{hatched} {boss}");
        if expected > 0 {
            assert_eq!(g.ally_t, if boss { 9. } else { 14. });
            for a in &mut g.allies {
                a.cd = 0.;
                a.x = 500.;
            }
            g.step(0., Input::default(), view()).unwrap();
            let wild: Vec<_> = g.shots.iter().filter(|s| s.owner == "wild").collect();
            assert_eq!(wild.len(), expected);
            assert!(wild.iter().all(|s| s.life == 1.1
                && s.dmg == 1.4
                && s.kind == dragon_game::combat::Kind::Flame));
            for a in &mut g.allies {
                a.x = -view().scale * 3.;
            }
            g.step(0., Input::default(), view()).unwrap();
            assert!(g.allies.is_empty());
        }
    }
}
