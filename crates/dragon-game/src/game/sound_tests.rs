use super::*;
fn game() -> Game {
    Game::first_flight(Viewport::from_screen(1000., 700., 1., 40., 0.62), 17)
}
#[test]
fn sounds_survive_steps_until_drained_and_never_change_gameplay_rng() {
    let mut a = game();
    let mut b = game();
    a.sound(sound::Event::Win);
    for _ in 0..100 {
        a.sound_state.quiet_hit();
    }
    for _ in 0..5 {
        let v = a.view;
        a.step(0.01, Input::default(), v).unwrap();
        b.step(0.01, Input::default(), v).unwrap();
    }
    assert_eq!(a.rng.next_unit(), b.rng.next_unit());
    assert_eq!(a.player.x, b.player.x);
    assert_eq!(a.player.y, b.player.y);
    assert_eq!(a.mission.t, b.mission.t);
    assert_eq!(a.drain_sound_events().next(), Some(sound::Event::Win));
    assert!(a.sound_events.is_empty());
    for _ in 0..1000 {
        a.sound(sound::Event::Thud);
    }
    assert_eq!(a.sound_events.len(), sound::CAPACITY);
}
#[test]
fn gem_combo_expiry_golden_collection_and_lost_gems() {
    let mut g = game();
    let v = g.view;
    let gem = Gem {
        x: g.player.x,
        y: g.player.y,
        r: 1.,
        life: 10.,
        value: 1.,
        ..Gem::default()
    };
    g.gems.push(gem.clone());
    g.step(0., Input::default(), v).unwrap();
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Gem(1)]
    );
    g.gems.push(Gem {
        golden: true,
        ..gem.clone()
    });
    g.gems.push(gem.clone());
    g.step(0., Input::default(), v).unwrap();
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Gem(2), sound::Event::Gold]
    );
    g.sound_state.tick(1.5);
    g.gems.push(gem.clone());
    g.gems.push(Gem {
        lost_one: true,
        ..gem
    });
    g.step(0., Input::default(), v).unwrap();
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Gem(1)]
    );
}
#[test]
fn follower_breath_is_silent_and_player_breath_follows_weapon_order() {
    let mut g = game();
    let d = DragonInfo {
        id: "wing".into(),
        kind: BreathKind::Frost,
        xp: 0.,
        bonded: false,
    };
    let origin = [g.player.x, g.player.y];
    g.breath(d.clone(), true, origin, &mut || 0.5);
    assert!(g.sound_events.is_empty());
    g.breath(d, false, origin, &mut || 0.5);
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Breath(BreathKind::Frost)]
    );
}
#[test]
fn abilities_are_silent_when_rejected_and_effects_precede_completion_cue() {
    let mut g = game();
    g.player.special_cd = 3.;
    g.special();
    assert!(g.sound_events.is_empty());
    g.player.special_cd = 0.;
    g.special();
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Shadow]
    );
    g.player.energy = 0.;
    g.cast(abilities::Spell::Naina);
    assert!(g.sound_events.is_empty());
    g.enemy_effects(
        vec![enemies::Event::Ring {
            position: [0., 0.],
            radius: 1.,
            duration: 0.4,
            kind: enemies::Kind::Jelly,
        }],
        g.view,
        &mut || 0.5,
    );
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Zap]
    );
}
fn original_function(name: &str) -> &'static str {
    let source = include_str!("../../../../web/dragon/index.html");
    let start = source.find(&format!("function {name}(")).unwrap();
    let tail = &source[start..];
    &tail[..tail.find("\nfunction ").unwrap()]
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn original_hit_enemy_and_defeat_sound_order_match() {
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for kind in [
        enemies::Kind::Cloud,
        enemies::Kind::Urgal,
        enemies::Kind::Slime,
        enemies::Kind::Chieftain,
    ] {
        for shot in [
            Shot {
                dmg: 0.6,
                ..Shot::default()
            },
            Shot {
                dmg: 1.,
                ..Shot::default()
            },
            Shot {
                dmg: 1.,
                kind: BreathKind::Hammer,
                ..Shot::default()
            },
            Shot {
                dmg: 100.,
                ..Shot::default()
            },
            Shot {
                dmg: 0.,
                mega: true,
                ..Shot::default()
            },
        ] {
            for guest in [false, true] {
                let mut g = game();
                g.spawn(kind, false, Some([300., 200.]), &mut || 0.5);
                g.mission.guest = guest;
                let e = &g.enemies[0];
                let d = kind.definition();
                let definition = json!({"ground":d.ground,"hop":d.hop,"gems":d.gems,"boss":d.boss,"armor":e.target.armor,"name":"fixture"});
                let target = json!({"x":e.target.x,"y":e.target.y,"r":e.target.r,"hp":e.target.hp,"maxHp":e.target.max_hp,"boss":e.target.boss,"type":serde_json::to_value(kind).unwrap(),"def":definition,"dead":false,"calm":false,"marked":0,"cracked":0});
                let mut shot_value = serde_json::to_value(&shot).unwrap();
                shot_value["owner"] = json!("bond");
                cases.push(json!({"e":target,"s":shot_value,"guest":guest}));
                g.hit(
                    0,
                    &shot,
                    projectiles::Authority::default(),
                    &mut vec![],
                    &mut || 0.5,
                );
                expected.push(
                    g.drain_sound_events()
                        .map(|e| format!("{e:?}"))
                        .collect::<Vec<_>>(),
                );
            }
        }
    }
    let script = format!(
        r#"let sounds=[],mission={{guest:false}},hitStop=0,remoteBy=-1,POOF={{}},rainbows=[],particles=[],texts=[],gems=[],me={{party:[]}},S=40,W=1000,topLimit=40,groundY=600,reduceMotion=false;const TAU=Math.PI*2;const sfx={{smash:()=>sounds.push('Smash'),thud:()=>sounds.push('Thud'),grr:()=>sounds.push('Grr'),poof:()=>sounds.push('Poof'),win:()=>sounds.push('Win')}};const MathRandom=Math.random;Math.random=()=>0.5;function rand(a,b){{return(a+b)/2}}function clamp(x,a,b){{return Math.max(a,Math.min(b,x))}}function pick(a){{return a[0]}}function poofAt(){{}}function burst(){{}}function confetti(){{}}function toast(){{}}function addXP(){{}}function makeGem(){{return{{}}}}function sibBossDmg(){{return 0}}function defeat(e,o){{e.dead=true;defeatFx(e.def,e.type,e.x,e.y,e.r,false)}}const room=null,ri=0;{}
{}
const cases={};print(JSON.stringify(cases.map(c=>{{sounds=[];mission.guest=c.guest;hitEnemy(c.e,c.s);return sounds}})));"#,
        original_function("hitEnemy"),
        original_function("defeatFx"),
        json!(cases)
    );
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
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Vec<Vec<String>> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(expected, actual);
}

#[test]
fn mounted_breath_and_each_weapon_emit_in_source_order() {
    for (name, path) in [
        ("shadow", Path::Shadow),
        ("storm", Path::Storm),
        ("guardian", Path::Guardian),
        ("forge", Path::Forge),
    ] {
        let mut g = game();
        let mut p = crate::profile::new_profile("rider3");
        p["build"] = json!({"path":name,"name":"Rider"});
        p["party"][0]["xp"] = json!(4000);
        p["craft"]["saddles"] = json!(true);
        g.profile = Some(p);
        let origin = [g.player.x, g.player.y];
        g.shoot_all(origin, &mut || 0.5);
        assert_eq!(
            g.drain_sound_events().collect::<Vec<_>>(),
            vec![
                sound::Event::Breath(BreathKind::Bond),
                sound::Event::Weapon(path)
            ]
        );
        g.shoot_all(origin, &mut || 0.5);
        assert!(g.sound_events.is_empty(), "cooldowns gate repeats");
    }
}
#[test]
fn specials_spells_and_daddio_emit_only_when_accepted() {
    for (name, cue) in [
        ("shadow", sound::Event::Shadow),
        ("storm", sound::Event::Smash),
        ("guardian", sound::Event::Spell),
        ("forge", sound::Event::Smash),
    ] {
        let mut g = game();
        let mut p = crate::profile::new_profile("rider3");
        p["build"] = json!({"path":name,"name":"Rider"});
        g.profile = Some(p);
        g.special();
        assert_eq!(g.drain_sound_events().collect::<Vec<_>>(), vec![cue]);
        g.special();
        assert!(g.sound_events.is_empty());
    }
    let mut g = game();
    g.cast(abilities::Spell::Naina);
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Spell]
    );
    g.player.energy = 0.;
    g.cast(abilities::Spell::Naina);
    assert!(g.sound_events.is_empty());
    let mut g = game();
    let mut p = crate::profile::new_profile("rider3");
    p["build"] = json!({"path":"forge","name":"Daddio"});
    g.profile = Some(p);
    g.dad_power(abilities::DadPower::Said);
    g.dad_power(abilities::DadPower::Joke);
    g.special();
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![
            sound::Event::DadBoom,
            sound::Event::Rimshot,
            sound::Event::MegaLaunch
        ]
    );
    for _ in 0..14 {
        g.step(0.05, Input::default(), g.view).unwrap();
    }
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::MegaBoom]
    );
}
#[test]
fn enemy_volley_sound_counts_and_order_match_source_calls() {
    use enemies::HazardKind as H;
    let mut g = game();
    let mut effects = Vec::new();
    for (kind, count) in [
        (H::Rock, 3),
        (H::Bubble, 3),
        (H::Ember, 6),
        (H::Fall, 5),
        (H::Spark, 5),
        (H::Dust, 1),
    ] {
        for _ in 0..count {
            effects.push(enemies::Event::Hazard(enemies::Hazard {
                kind,
                position: [300., 100.],
                velocity: [0., 0.],
                gravity: 0.,
                radius: 1.,
                life: 1.,
                warn: 0.,
            }));
        }
    }
    g.enemy_effects(effects, g.view, &mut || 0.5);
    assert_eq!(
        g.drain_sound_events().collect::<Vec<_>>(),
        vec![
            sound::Event::Throw,
            sound::Event::Throw,
            sound::Event::Throw,
            sound::Event::Throw,
            sound::Event::Smash,
            sound::Event::Smash
        ]
    );
}

fn guest() -> crate::coop::Session {
    let mut s = crate::coop::Session::new(
        game(),
        "guest".into(),
        1,
        "run1".into(),
        crate::authority::Authority {
            guest: true,
            host_peer: Some("host".into()),
            host_rider: 0,
            epoch: 0,
        },
    )
    .unwrap();
    s.bind_peer("host".into(), 0, false).unwrap();
    s
}
fn snapshot(s: &crate::coop::Session) -> Value {
    json!({"r":0,"mid":s.game.definition.id,"run":"run1","epoch":0,"next":0,"wave":s.game.mission.wave,"lg":u8::from(s.game.mission.leg==Leg::Fly),"sp":s.game.mission.stop,"pr":0,"total":0,"kills":0,"done":0,"en":[],"hz":[],"st":[],"ob":null,"q":[]})
}
#[test]
fn guest_snapshot_cues_are_ordered_idempotent_and_authorized() {
    let mut s = guest();
    let mut d = snapshot(&s);
    assert!(s.receive("host", "world", &d, 0.));
    assert_eq!(
        s.game.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Wave]
    ); // Source catch-up clamps initial wave -1 to 0.
    d["wave"] = json!(2);
    d["lg"] = json!(1);
    d["sp"] = json!(2);
    s.game.mission.carry = 5.;
    assert!(s.receive("host", "world", &d, 1.));
    assert_eq!(
        s.game.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Wave, sound::Event::Gold]
    );
    assert!(s.receive("host", "world", &d, 2.));
    assert!(s.game.sound_events.is_empty());
    d["done"] = json!(2);
    assert!(s.receive("host", "world", &d, 3.));
    assert_eq!(
        s.game.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Wrong]
    );
    assert!(s.receive("host", "world", &d, 4.));
    assert!(s.game.sound_events.is_empty());
    d["done"] = json!(1);
    s.game.mission.carry = 3.;
    assert!(!s.receive("intruder", "world", &d, 5.));
    assert!(s.game.sound_events.is_empty());
    assert!(s.receive("host", "world", &d, 6.));
    assert_eq!(
        s.game.drain_sound_events().collect::<Vec<_>>(),
        vec![sound::Event::Gold, sound::Event::Win]
    );
    assert!(s.receive("host", "world", &d, 7.));
    assert!(s.game.sound_events.is_empty());
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI"]
fn original_guest_snapshot_announcement_sequences_match() {
    let mut s = guest();
    let base = snapshot(&s);
    let initial = json!({"wave":s.game.mission.wave,"leg":if s.game.mission.leg==Leg::Fly{"fly"}else{"fight"},"stop":s.game.mission.stop,"def":{"id":s.game.definition.id,"waves":vec![0;s.game.definition.waves.len()]},"guest":true,"hostPeer":"host","hostRider":0,"run":"run1","authority":0,"done":false,"failT":0,"carry":0,"banked":0});
    let mut packets = vec![base.clone(), base.clone()];
    let mut changed = base.clone();
    changed["wave"] = json!(2);
    changed["lg"] = json!(1);
    changed["sp"] = json!(2);
    packets.extend([changed.clone(), changed.clone()]);
    changed["done"] = json!(2);
    packets.extend([changed.clone(), changed.clone()]);
    changed["done"] = json!(1);
    packets.extend([changed.clone(), changed]);
    let mut expected = Vec::new();
    let mut cases = Vec::new();
    for (i, d) in packets.iter().enumerate() {
        let carry = if i == 2 {
            5.
        } else if i == 6 {
            3.
        } else {
            s.game.mission.carry
        };
        s.game.mission.carry = carry;
        assert!(s.receive("host", "world", d, i as f64));
        expected.push(
            s.game
                .drain_sound_events()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>(),
        );
        cases.push(json!({"d":d,"carry":carry}));
    }
    let script = format!(
        r#"let mission={},sounds=[],uidN=0,time=0,enemies=[],hazards=[],strikes=[],texts=[];const STATIC=true,S=40,W=1000,topLimit=40,groundY=600,reduceMotion=false,P={{x:50,y:50}},R={{dragon:'Dragon'}},me={{earned:0}},ENEMIES={{}},usedHaz=new Set(),usedStrike=new Set(),OB_KINDS=[],OB_WHATS=[];const sfx={{wave:()=>sounds.push('Wave'),wrong:()=>sounds.push('Wrong'),win:()=>sounds.push('Win'),gold:()=>sounds.push('Gold')}};function clamp(x,a,b){{return Math.max(a,Math.min(b,x))}}function pick(a){{return a[0]}}function say(){{}}function caption(){{}}function confetti(){{}}function stopLine(){{return ''}}function stopOf(d,i){{return [i]}}function placeName(i){{return i}}function markDirty(){{}} {}
{}
const cases={};print(JSON.stringify(cases.map(c=>{{sounds=[];mission.carry=c.carry;onWorld(c.d,'host');return sounds}})));"#,
        initial,
        original_function("bankGems"),
        original_function("onWorld"),
        json!(cases)
    );
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
        .write_all(format!("{}\n", serde_json::to_string(&script).unwrap()).as_bytes())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{} {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let actual: Vec<Vec<String>> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(expected, actual);
}
