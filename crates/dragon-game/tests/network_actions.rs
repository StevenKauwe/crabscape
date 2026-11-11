#[cfg(target_os = "macos")]
use dragon_game::missions::Leg;
use dragon_game::{
    authority::Authority,
    enemies::{Enemy, Kind, MissionScaling, World},
    network_actions::*,
    network_world::{GuestWorld, Mirror, Viewport},
};
use serde_json::{Value, json};
fn view() -> Viewport {
    Viewport {
        width: 1000.,
        top: 50.,
        ground: 650.,
        scale: 20.,
    }
}
fn world(guest: bool) -> GuestWorld {
    let mut w = GuestWorld::new(
        "m1".into(),
        4,
        Authority {
            guest,
            host_peer: Some("host".into()),
            host_rider: 1,
            epoch: 2,
        },
    );
    w.run = "run".into();
    w
}
fn sender(peer: &str, rider: usize) -> Option<Sender<'_>> {
    Some(Sender {
        peer,
        rider,
        daddio: rider == 1,
    })
}
fn enemy(kind: Kind, id: u64) -> Enemy {
    Enemy::spawn(
        kind,
        id,
        false,
        Some([100., 100.]),
        &World {
            width: 1000.,
            ground_y: 650.,
            top: 50.,
            sky_bottom: 600.,
            scale: 20.,
            player: [500., 300.],
            aim: [500., 300.],
            quiet: false,
            safe: false,
            dragon_scale: 1.,
        },
        MissionScaling::default(),
        &mut || 0.5,
    )
}
fn hit() -> Value {
    json!({"r":1,"mid":"m1","run":"run","epoch":2,"uid":7,"dmg":5,"kind":"flame","slow":1,"stun":2,"mega":1,"mk":1,"cr":1})
}
fn context() -> PresenceContext<'static> {
    PresenceContext {
        local_rider: 0,
        playing: true,
        local_mission: Some("m1"),
        local_host: true,
        objective_carrier: Some(1),
        player: [100., 100.],
        player_out: false,
        scale: 20.,
    }
}
fn presence(cast: i32, word: &str) -> Value {
    json!({"r":1,"s":"play","x":0.25,"y":0.5,"h":4,"cs":[cast,word],"ms":["m1",0,2,3,"run",0,2]})
}
#[test]
fn action_admission_requires_binding_and_current_run_epoch() {
    let host = world(false);
    let enemies = vec![enemy(Kind::Cloud, 7)];
    for variant in 0..7 {
        let mut d = hit();
        let mut s = sender("guest", 1);
        match variant {
            0 => s = None,
            1 => d["r"] = json!(2),
            2 => d["run"] = json!("old"),
            3 => d["epoch"] = json!(1),
            4 => d["uid"] = json!("7"),
            5 => d["uid"] = json!(7.5),
            _ => d["mid"] = json!("m2"),
        };
        assert!(remote_hit(&host, &enemies, s, &d).is_none());
    }
    let guest = world(true);
    assert!(remote_hit(&guest, &enemies, sender("guest", 1), &hit()).is_none());
    let defeat = json!({"r":1,"mid":"m1","run":"run","epoch":2,"uid":7,"type":"cloud","x":0.2,"y":0.3,"by":0});
    assert!(remote_defeat(&guest, sender("not-host", 1), &defeat, 0, view()).is_none());
    assert_eq!(
        remote_defeat(&guest, sender("host", 1), &defeat, 0, view())
            .unwrap()
            .len(),
        3
    );
}
#[test]
fn takeover_restores_source_state_and_moves_mirrors_without_duplicate_rng() {
    let mut w = world(true);
    w.last_queue = vec!["cloud".into(), "bat".into()];
    w.mission.prog = 0.37;
    let mut e = enemy(Kind::Sprite, 7);
    e.destination = Some([5., 6.]);
    e.windup = 2.;
    e.pre = 0.8;
    w.enemies.push(Mirror {
        enemy: e,
        destination: [1., 2.],
    });
    let mut draws = 0;
    let t = become_host(&mut w, TakeoverReason::Lost, &mut || {
        draws += 1;
        0.25
    })
    .unwrap();
    assert_eq!(draws, 2);
    assert_eq!(w.authority.epoch, 3);
    assert!(!w.authority.guest && !w.mission.guest);
    assert!(w.enemies.is_empty());
    assert_eq!(w.mission.total, 3);
    assert_eq!(w.mission.leg_from, 0.37);
    assert_eq!(w.mission.leg_t, 4.);
    assert_eq!(t.enemies[0].cool, 1.875);
    assert_eq!(t.enemies[0].cool2, 4.75);
    assert_eq!(t.enemies[0].pre, 0.8);
    assert_eq!(t.enemies[0].destination, None);
    assert!(become_host(&mut w, TakeoverReason::Lost, &mut || panic!("not called")).is_none());
}
#[test]
fn presence_seeds_casts_then_emits_ordered_gameplay_once() {
    let mut g = None;
    assert!(
        ingest_presence(
            &mut g,
            sender("guest", 1),
            &presence(4, "naina"),
            context(),
            |m| m == "m1"
        )
        .unwrap()
        .is_empty()
    );
    g.as_mut().unwrap().position = Some([100., 100.]);
    let mut p = presence(5, "guard");
    p["h"] = json!(3);
    let effects =
        ingest_presence(&mut g, sender("guest", 1), &p, context(), |m| m == "m1").unwrap();
    assert_eq!(
        effects,
        vec![PresenceEffect::DropPieceBonk, PresenceEffect::GuardianHeal]
    );
    assert!(
        ingest_presence(&mut g, sender("guest", 1), &p, context(), |m| m == "m1")
            .unwrap()
            .is_empty()
    );
    assert!(ingest_presence(&mut g, sender("stranger", 1), &p, context(), |_| true).is_none());
    let mut p = presence(6, "thuri");
    p["ms"] = Value::Null;
    assert_eq!(
        ingest_presence(&mut g, sender("guest", 1), &p, context(), |_| true).unwrap(),
        vec![
            PresenceEffect::ApplyTeamSpell(dragon_game::abilities::Spell::Thuri),
            PresenceEffect::AnnounceSpell(dragon_game::abilities::Spell::Thuri)
        ]
    );
}
#[test]
fn explicit_handoff_chooses_lowest_rider_not_lexical_peer() {
    let w = world(true);
    let mut ghosts = Vec::new();
    for (peer, rider) in [("zzz", 2), ("aaa", 3)] {
        let mut g = None;
        let mut p = presence(0, "");
        p["r"] = json!(rider);
        ingest_presence(&mut g, sender(peer, rider), &p, context(), |_| true).unwrap();
        ghosts.push(g.unwrap());
    }
    assert_eq!(
        handoff(&w, sender("host", 1), &hit(), &ghosts, 4, false),
        Some(Handoff::Follow {
            rider: 2,
            peer: Some("zzz".into())
        })
    );
    assert_eq!(
        handoff(&w, sender("host", 1), &hit(), &ghosts, 0, false),
        Some(Handoff::BecomeHost)
    );
    assert!(handoff(&w, sender("not-host", 1), &hit(), &ghosts, 0, false).is_none());
}
#[cfg(target_os = "macos")]
fn js(script: &str) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut c =
        Command::new("/System/Library/Frameworks/JavaScriptCore.framework/Versions/A/Helpers/jsc")
            .args(["-e", "eval(JSON.parse(readline()))"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(format!("{}\n", json!(script)).as_bytes())
        .unwrap();
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[cfg(target_os = "macos")]
fn equivalent(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-9,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equivalent(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, a) in a {
                equivalent(a, &b[k], &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
#[cfg(target_os = "macos")]
fn extract<'a>(s: &'a str, start: &str, end: &str) -> &'a str {
    let i = s.find(start).unwrap();
    let j = i + s[i..].find(end).unwrap();
    &s[i..j]
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires JavaScriptCore; actual source remote hit and defeat handlers"]
fn actual_js_remote_actions_match_valid_wire_boundaries() {
    let src = include_str!("../../../web/dragon/index.html");
    let functions = extract(src, "function onRemoteHit(", "// A bump:");
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for flags in 0..128 {
        let mut w = world(false);
        w.authority.guest = flags & 1 != 0;
        let mut d = hit();
        d["dmg"] = json!(if flags & 2 != 0 { 99 } else { -1 });
        d["slow"] = json!(9);
        d["stun"] = json!(-2);
        d["calm"] = json!(flags & 4 != 0);
        d["mega"] = json!(if flags & 8 != 0 { 1 } else { 0 });
        if flags & 16 != 0 {
            d["epoch"] = json!(1);
        }
        let mut e = enemy(
            if flags & 32 != 0 {
                Kind::Thunderhead
            } else {
                Kind::Cloud
            },
            7,
        );
        e.target.dead = flags & 64 != 0;
        let result = remote_hit(&w, std::slice::from_ref(&e), sender("guest", 1), &d);
        let action = result
            .as_ref()
            .and_then(|r| r.action.as_ref())
            .map(|a| match a {
                HitAction::Calm => json!(["calm", 1]),
                HitAction::Hit(s) => json!([
                    "hit", 1, s.dmg, s.kind, s.slow, s.stun, s.mega, s.mark, s.crack, s.owner
                ]),
            });
        expected.push(json!({"hits":usize::from(result.is_some()),"by":-1,"action":action}));
        cases.push(json!({"guest":w.authority.guest,"data":d,"enemy":{"uid":7,"dead":e.target.dead,"calm":false,"boss":e.target.boss}}));
    }
    let script = format!(
        r#"const STATIC=true,RIDERS=[{{name:'kid'}},{{name:'Daddio'}}],isDaddio=r=>r?.name==='Daddio',clamp=(v,a,b)=>Math.max(a,Math.min(b,v));let mission,enemies,remoteHits,remoteBy,action;function defeat(){{action=['calm',remoteBy];}}function hitEnemy(e,s){{action=['hit',remoteBy,s.dmg,s.kind,s.slow,s.stun,s.mega,s.mark,s.crack,s.owner];}}{functions}const cases={cases};print(JSON.stringify(cases.map(c=>{{mission={{guest:c.guest,def:{{id:'m1'}},run:'run',authority:2}};enemies=[c.enemy];remoteHits=0;remoteBy=-1;action=null;onRemoteHit(c.data);return {{hits:remoteHits,by:remoteBy,action}};}})));"#,
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "hit");
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for (kind, ti) in [(Kind::Cloud, "cloud"), (Kind::Thunderhead, "thunderhead")] {
        for flags in 0..16 {
            let w = world(true);
            let peer = if flags & 1 != 0 { "host" } else { "wrong" };
            let d = json!({"r":1,"mid":"m1","run":if flags&2!=0{"run"}else{"old"},"epoch":2,"uid":7,"type":ti,"x":-3,"y":9,"calmed":flags&4!=0,"by":if flags&8!=0{0}else{1}});
            let effects = remote_defeat(&w, sender(peer, 1), &d, 0, view());
            let out=effects.map_or(json!({"left":1,"fx":null,"kills":0,"xp":0}),|e|{let credit=e.len()==3;json!({"left":0,"fx":[ti,0.,770.,20.*kind.definition().radius,flags&4!=0],"kills":usize::from(credit),"xp":if credit{4}else{0}})});
            expected.push(out);
            cases.push(json!({"peer":peer,"data":d}));
        }
    }
    let script = format!(
        r#"const STATIC=true,W=1000,topLimit=50,groundY=650,S=20,ri=0,ENEMIES={{cloud:{{r:.95,boss:false}},thunderhead:{{r:2.2,boss:true}}}},clamp=(v,a,b)=>Math.max(a,Math.min(b,v));let mission,enemies,fx,mStats,xp;function defeatFx(d,t,x,y,r,c){{fx=[t,x,y,r,c];}}function mainDragon(){{return null;}}function addXP(d,n){{xp+=n;}}{functions}const cases={cases};print(JSON.stringify(cases.map(c=>{{mission={{guest:true,hostPeer:'host',def:{{id:'m1'}},run:'run',authority:2}};enemies=[{{uid:7}}];fx=null;mStats={{kills:0}};xp=0;onRemoteDefeat(c.data,c.peer);return {{left:enemies.length,fx,kills:mStats.kills,xp}};}})));"#,
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "defeat");
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires JavaScriptCore; actual source becomeHost"]
fn actual_js_takeover_matches_queue_timers_and_rng_order() {
    let src = include_str!("../../../web/dragon/index.html");
    let function = extract(src, "function becomeHost(", "/* ---------- Missions:");
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for flags in 0..8 {
        let mut w = world(true);
        w.last_queue = vec!["cloud".into(), "bat".into()];
        w.mission.leg = if flags & 1 != 0 { Leg::Fly } else { Leg::Fight };
        w.mission.total = if flags & 2 != 0 { 9 } else { 0 };
        w.mission.prog = 0.4;
        let mut sprite = enemy(Kind::Sprite, 1);
        sprite.destination = Some([2., 3.]);
        sprite.windup = 1.;
        sprite.pre = 0.4;
        sprite.target.calm = flags & 4 != 0;
        let e = enemy(Kind::Cloud, 2);
        w.enemies = vec![
            Mirror {
                enemy: sprite,
                destination: [4., 5.],
            },
            Mirror {
                enemy: e,
                destination: [6., 7.],
            },
        ];
        cases.push(json!({"leg":if w.mission.leg==Leg::Fly{"fly"}else{"fight"},"total":w.mission.total,"calm":flags&4!=0}));
        let mut n = 0;
        let result = become_host(&mut w, TakeoverReason::Handoff, &mut || {
            n += 1;
            n as f64 / 10.
        })
        .unwrap();
        let m = &w.mission;
        expected.push(json!({"guest":w.authority.guest,"peer":w.authority.host_peer,"epoch":w.authority.epoch,"queue":m.queue,"spawn":m.spawn_t,"legT":m.leg_t,"legMax":m.leg_max,"legFrom":m.leg_from,"trick":m.trick_t,"total":m.total,"draws":n,"en":result.enemies.iter().map(|e|json!([e.entered,e.cool,e.cool2,e.windup,e.destination.is_some()])).collect::<Vec<_>>() }));
    }
    let script = format!(
        r#"const STATIC=true,RIDERS=[{{name:'kid'}},{{name:'host'}}];let mission,enemies,n;function rand(a,b){{n++;return a+n/10*(b-a);}}function toast(){{}}{function}const cases={cases};print(JSON.stringify(cases.map(c=>{{mission={{guest:true,hostPeer:'host',hostRider:1,authority:2,lastQueue:['cloud','bat'],leg:c.leg,prog:.4,total:c.total,legT:0,legMax:0,legFrom:0,trickT:0}};enemies=[{{type:'sprite',calm:c.calm,extra:false,mx:4,my:5,tx:2}},{{type:'cloud',extra:false,calm:false,mx:6,my:7}}];n=0;becomeHost('handoff');const M=mission;return {{guest:M.guest,peer:M.hostPeer,epoch:M.authority,queue:M.queue,spawn:M.spawnT,legT:M.legT,legMax:M.legMax,legFrom:M.legFrom,trick:M.trickT,total:M.total,draws:n,en:enemies.map(e=>[e.entered,e.cool,e.cool2,e.windup,e.tx!==undefined])}};}})));"#,
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "takeover");
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires JavaScriptCore; actual source live presence gameplay projection"]
fn actual_js_presence_gameplay_matches_monotonic_casts_and_progress() {
    let src = include_str!("../../../web/dragon/index.html");
    let function = extract(src, "function ingestPeer(", "function syncGhosts(");
    let mut cases = Vec::new();
    for (i, word) in [
        "naina", "naina", "guard", "mega", "joke:3", "said", "thuri", "guard", "naina",
    ]
    .into_iter()
    .enumerate()
    {
        let mut d = presence(i as i32, word);
        d["h"] = json!(if i >= 2 { 3 } else { 4 });
        if i >= 6 {
            d["ms"][0] = json!("m2");
            d["ms"][4] = json!("new");
        }
        if i == 7 {
            d["s"] = json!("frozen");
        }
        if i == 8 {
            d["ms"] = Value::Null;
        }
        cases.push(d);
    }
    cases.push(cases[8].clone());
    let mut g = None;
    let mut expected = Vec::new();
    for d in &cases {
        let effects = ingest_presence(&mut g, sender("guest", 1), d, context(), |m| {
            m == "m1" || m == "m2"
        })
        .unwrap();
        let events = effects
            .iter()
            .filter_map(|e| {
                Some(match e {
                    PresenceEffect::DropPieceBonk => "drop".into(),
                    PresenceEffect::ApplyTeamSpell(s) => format!("apply:{}", s.key()),
                    PresenceEffect::EpicJoke(_) => "joke".into(),
                    PresenceEffect::EpicMega(_) => "mega".into(),
                    PresenceEffect::DelayedMegaBoom => "delay".into(),
                    PresenceEffect::EpicSaid(_) => "said".into(),
                    PresenceEffect::GuardianHeal => "guard".into(),
                    PresenceEffect::AnnounceFrozen => "frozen".into(),
                    _ => return None,
                })
            })
            .collect::<Vec<String>>();
        let ghost = g.as_mut().unwrap();
        expected.push(json!({"r":ghost.rider,"status":ghost.status,"target":ghost.target,"facing":ghost.facing,"out":ghost.out,"hearts":ghost.hearts,"ms":ghost.mission,"run":ghost.run,"host":ghost.host,"epoch":ghost.epoch,"kills":ghost.wave_kills,"boss":ghost.boss_damage,"cast":ghost.cast_count,"frozen":ghost.was_frozen,"events":events}));
        ghost.position = Some([100., 100.]);
    }
    let script = format!(
        r#"const RIDERS=[{{name:'R0',color:'#fff'}},{{name:'R1',color:'#fff'}}],ri=0,state='play',S=20,P={{x:100,y:100,out:false,hearts:3,shield:0}},R={{}},KINDS={{bond:{{}}}},WORDS={{naina:{{team:true,word:'Naína'}},thuri:{{team:true,word:'Thuri'}}}},DAD_JOKES=Array(9).fill('joke'),MEGA_DELAY=.6,TEST_FLIGHT=false;let events=[],ghosts=new Map(),partyPick={{at:0}},partyCount=0;let mission={{def:{{id:'m1'}},guest:false,run:'local-run',ob:{{carrier:1}}}};const clamp=(n,a,b)=>Math.max(a,Math.min(b,n)),safeColor=(c,d)=>d,missionById=m=>['m1','m2'].includes(m),dist=(a,b,c,d)=>Math.hypot(a-c,b-d),maxHearts=()=>5;function dropPiece(){{events.push('drop');}}function applySpell(w){{events.push('apply:'+w);}}function toast(s){{if(s.includes('got frozen'))events.push('frozen');}}function epicJoke(){{events.push('joke');}}function epicMega(){{events.push('mega');}}function epicSaid(){{events.push('said');}}function setTimeout(){{events.push('delay');}}function burst(){{events.push('guard');}}function updateHud(){{}}const texts=[];{function}const cases={cases};print(JSON.stringify(cases.map(d=>{{events=[];ingestPeer('guest',d,false);const g=ghosts.get('guest');const out={{r:g.rider,status:g.status,target:[g.tx,g.ty],facing:g.facing,out:g.out,hearts:g.hearts,ms:g.ms,run:g.run,host:g.msHost,epoch:g.authority,kills:{{...g.waveKills}},boss:g.bossDmg,cast:g.castN,frozen:g.wasFrozen,events}};g.x=100;g.y=100;return out;}})));"#,
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "presence");
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "requires JavaScriptCore; actual source explicit handoff callback"]
fn actual_js_handoff_matches_rider_order_and_out_filter() {
    let src = include_str!("../../../web/dragon/index.html");
    let body = extract(src, "        if (!mission?.guest || !m.data", "      });");
    let active = extract(src, "function activeOthers()", "function updateHazards(");
    let mut cases = Vec::new();
    let mut expected = Vec::new();
    for flags in 0..64 {
        let mut ghosts = Vec::new();
        let mut json_ghosts = Vec::new();
        for (i, (peer, rider)) in [("host", 1), ("zzz", 2), ("aaa", 3)]
            .into_iter()
            .enumerate()
        {
            let mut g = None;
            let mut d = presence(0, "");
            d["r"] = json!(rider);
            d["o"] = json!(if flags & (1 << i) != 0 { 1 } else { 0 });
            d["s"] = json!(if flags & (1 << (i + 3)) != 0 {
                "map"
            } else {
                "play"
            });
            ingest_presence(&mut g, sender(peer, rider), &d, context(), |_| true).unwrap();
            let g = g.unwrap();
            json_ghosts.push(
                json!([peer,{"rider":rider,"out":g.out,"status":g.status,"ms":"m1","viaDb":false}]),
            );
            ghosts.push(g);
        }
        let result = handoff(&world(true), sender("host", 1), &hit(), &ghosts, 4, false).unwrap();
        expected.push(match result {
            Handoff::BecomeHost => json!([true, 1, "host"]),
            Handoff::Follow { rider, peer } => json!([false, rider, peer]),
        });
        cases.push(json!(json_ghosts));
    }
    let script = format!(
        r#"const STATIC=true,ri=4,P={{out:false}};let mission,ghosts,takeover;function becomeHost(){{takeover=true;}}{active}function handle(m){{{body}}}const cases={cases};print(JSON.stringify(cases.map(g=>{{mission={{guest:true,def:{{id:'m1'}},hostPeer:'host',hostRider:1,run:'run',authority:2}};ghosts=new Map(g);takeover=false;handle({{peer:'host',data:{{r:1,mid:'m1',run:'run',epoch:2}}}});return [takeover,mission.hostRider,mission.hostPeer];}})));"#,
        cases = json!(cases)
    );
    equivalent(&json!(expected), &js(&script), "handoff");
}
