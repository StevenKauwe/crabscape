use dragon_game::{
    adventure::{Adventure, Clock, Error, Event},
    flight::{Input, Viewport},
    profile, progression,
};
use serde_json::{Value, json};
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
fn clock(now: u64) -> Clock {
    Clock {
        now_ms: now,
        day: 0,
    }
}
fn built(raw: Value) -> Value {
    let mut p = profile::normalize_profile(&raw, "rider3", 2, String::new).unwrap();
    if p["build"].is_null() {
        p["build"] =
            profile::clean_build(&json!({"name":"Nova","dname":"Ember","path":"forge"})).unwrap();
    }
    p
}
fn start(raw: Value) -> Adventure {
    Adventure::new(built(raw), vec![], view(), 42, 2).unwrap().0
}
#[test]
fn built_boundary_intro_and_shared_briefing_pause_are_real_transitions() {
    assert!(matches!(
        Adventure::new(profile::new_profile("rider3"), vec![], view(), 42, 2),
        Err(Error::BuilderRequired)
    ));
    let p = built(json!({}));
    let original = p.clone();
    let (mut a, _) = Adventure::new(p, vec![json!({"cleared":["m3"]})], view(), 42, 2).unwrap();
    assert_eq!(a.state(), "story");
    assert_eq!(a.story_id(), Some("intro"));
    assert_eq!(a.profile()["seenIntro"], false);
    assert!(a.close_story(false).unwrap().is_empty());
    assert_eq!(a.story_id(), Some("intro"));
    a.close_story(true).unwrap();
    assert_eq!(a.state(), "party");
    assert_eq!(a.profile()["seenIntro"], true);
    a.to_map().unwrap();
    assert!(a.mission_unlocked("m4"));
    assert!(!a.mission_unlocked("m5"));
    assert_eq!(a.choose_mission("m5", false), Err(Error::LockedMission));
    let e = a.choose_mission("m4", false).unwrap();
    assert_eq!(a.state(), "story");
    assert!(e.iter().any(
        |e| matches!(e,Event::Story{page,..}if page["clip"]=="brief-m4"&&page["next"].is_string())
    ));
    a.close_story(true).unwrap();
    assert_eq!(a.state(), "play");
    assert_eq!(a.profile()["briefed"], json!(["m4"]));
    assert_eq!(a.profile()["lastRealm"], "hollow");
    a.pause();
    let time = a.game.as_ref().unwrap().mission.t;
    a.tick(0.05, Input::default(), view(), clock(100)).unwrap();
    assert_eq!(a.game.as_ref().unwrap().mission.t, time);
    a.resume();
    a.tick(0.05, Input::default(), view(), clock(150)).unwrap();
    assert!(a.game.as_ref().unwrap().mission.t > time);
    assert_eq!(a.initial_profile(), &original);
}
#[test]
fn boss_completion_story_rank_results_egg_gate_hatch_roost_chain() {
    let mut a = start(json!({"seenIntro":true,"xpv":2,"skills":{},"cleared":["m3"]}));
    a.choose_mission("m4", false).unwrap();
    a.close_story(true).unwrap();
    // The Game's mission/combat unit tests prove combat; this synthetic terminal
    // state exercises the real endMission integration and callback chain.
    let g = a.game.as_mut().unwrap();
    g.mission.done = true;
    g.mission.end_t = 0.01;
    a.tick(0.02, Input::default(), view(), clock(1000)).unwrap();
    assert!(a.game.is_none());
    assert_eq!(a.story_id(), Some("m4"));
    assert_eq!(a.profile()["stars"], 1.0);
    assert_eq!(a.profile()["eggs"], 1.0);
    assert!(a.result.as_ref().unwrap().won);
    a.close_story(true).unwrap();
    assert_eq!(a.story_id(), Some("rank"));
    a.close_story(true).unwrap();
    assert_eq!(a.state(), "results");
    a.wake_egg(clock(2000)).unwrap();
    assert_eq!(a.state(), "riddle");
    a.tick(0., Input::default(), view(), clock(3000)).unwrap();
    let mut now = 3000;
    for i in 0..5 {
        let answer = a.gate.question.unwrap().n.to_string();
        for c in answer.chars() {
            a.times_key(&c.to_string());
        }
        a.answer_times(clock(now)).unwrap();
        now += if i == 4 { 900 } else { 600 };
        a.tick(0., Input::default(), view(), clock(now)).unwrap();
    }
    assert_eq!(a.state(), "hatch");
    assert!(a.roost.pending_baby.is_some());
    a.finish_hatch("Fern", None, clock(now + 1)).unwrap();
    assert_eq!(a.state(), "roost");
    assert_eq!(a.profile()["party"].as_array().unwrap().len(), 2);
    assert_eq!(a.profile()["party"][1]["name"], "Fern");
    assert_eq!(a.profile()["hatched"], 1.0);
    assert_eq!(a.profile()["eggs"], 0.0);
    assert!(a.profile()["seenMs"].as_array().unwrap().is_empty());
    a.close_milestone().unwrap();
    assert_eq!(a.profile()["seenMs"], json!(["first"]));
}
#[test]
fn growth_chain_part_two_and_skill_lesson_keep_source_order() {
    let mut a = start(
        json!({"seenIntro":true,"xpv":2,"skills":{},"stars":10,"craft":{"saddles":true},"party":[{"bonded":true,"xp":5000}],"cleared":["m12"]}),
    );
    assert_eq!(a.story_id(), Some("part2"));
    assert_eq!(a.profile()["seenPart2"], true);
    a.close_story(true).unwrap();
    assert_eq!(a.state(), "party");
    a.to_roost().unwrap();
    for (id, count) in [("glider", 1), ("grown", 2), ("ride", 3), ("mighty", 4)] {
        assert_eq!(a.story_id(), Some("grow"));
        assert_eq!(a.profile()["seenGrow"].as_array().unwrap().len(), count);
        assert_eq!(a.profile()["seenGrow"][count - 1], id);
        a.close_story(true).unwrap();
    }
    assert_eq!(a.state(), "roost");
    a.roost_command(progression::Command::LearnSkill("rider".into()), clock(9))
        .unwrap();
    assert_eq!(a.story_id(), Some("lesson"));
    assert_eq!(a.profile()["skills"]["rider"], 1);
    a.close_story(true).unwrap();
    assert_eq!(a.state(), "skills");
}
fn block(start: &str, end: &str, inclusive: bool) -> &'static str {
    let s = include_str!("../../../web/dragon/index.html");
    let t = &s[s.find(start).unwrap()..];
    &t[..t.find(end).unwrap() + if inclusive { end.len() } else { 0 }]
}
fn flow_events(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| {
            Some(match e {
                Event::Dirty => "dirty".into(),
                Event::Flush => "flush".into(),
                Event::Blur => "blur".into(),
                Event::StopNarration => "stop".into(),
                Event::ClearField => "clear".into(),
                Event::UpdateHud => "hud".into(),
                Event::Screen { name } => format!("screen:{}", name.as_deref().unwrap_or("play")),
                Event::Story { id, .. } => format!("story:{id}"),
                Event::Sound { name } => format!("sound:{name}"),
                _ => return None,
            })
        })
        .collect()
}
#[test]
#[ignore = "requires macOS JavaScriptCore CLI; run explicitly on a Mac"]
fn original_navigation_callback_order_matches() {
    let profiles:Vec<_>=[json!({}),json!({"seenIntro":true,"cleared":["m12"]}),json!({"seenIntro":true,"xpv":2,"craft":{"saddles":true},"party":[{"bonded":true,"xp":5000}]})].into_iter().map(built).collect();
    let mut script = String::from(
        r#"let me,state,mission,realmIdx,mapSig,roostSig,healNote,partyReady,partyCount,partySig,storyId,storyThen,storyOpenAt,storyGateT,events;const narrateOn=false;const $=()=>({});const plog=()=>{},clearTimeout=()=>{},paintBackground=()=>{},refreshMap=()=>{},refreshRoost=()=>{},refreshPartyScreen=()=>{},checkMilestones=()=>{},requestAnimationFrame=()=>{},renderViewPills=()=>{},releaseControls=()=>{},pressed={clear(){}};
const markDirty=()=>events.push('dirty'),flushSave=()=>events.push('flush'),blurActive=()=>events.push('blur'),narrStop=()=>events.push('stop'),clearField=()=>events.push('clear'),updateHud=()=>events.push('hud'),showOnly=s=>events.push('screen:'+(s||'play'));
const showStory=(id,then,kicker,page)=>{storyId=id;storyThen=then;state='story';storyOpenAt=0;events.push('story:'+id,'screen:storyScreen','sound:spell');};const bonded=()=>me.party[0],forge=()=>me.craft;const RIDE_LEVEL=8;
"#,
    );
    script.push_str(block("const STAGES = [", "\n];", true));
    script.push_str(block("const GROW_SCENES = [", "\n];", true));
    for start in [
        "const XP_K =",
        "const levelOf =",
        "function stageOf(",
        "const riding =",
        "const part2Due =",
        "function showPart2(",
    ] {
        script.push_str(block(start, "\n", true));
    }
    for start in [
        "function toParty(",
        "function toMap(",
        "function showGrowScene(",
        "function toRoost(",
        "function closeStory(",
    ] {
        script.push_str(block(start, "\n}", true));
    }
    script.push_str(&format!(r#"const profiles={};print(JSON.stringify(profiles.map(p=>{{me=p;events=[];state='menu';toParty();if(!me.seenIntro)showStory('intro',toParty);const steps=[events.slice()];for(const op of ['close','map','roost','close','close','close','close','map']){{events=[];if(op==='close')closeStory();if(op==='map')toMap();if(op==='roost')toRoost();steps.push(events.slice());}}return{{steps,me,state}};}})));"#,serde_json::to_string(&profiles).unwrap()));
    let js: Vec<Value> = serde_json::from_slice(&run_js(&script)).unwrap();
    for (p, j) in profiles.into_iter().zip(js) {
        let (mut a, e) = Adventure::new(p, vec![], view(), 42, 2).unwrap();
        let mut actual = vec![flow_events(&e)];
        for op in [
            "close", "map", "roost", "close", "close", "close", "close", "map",
        ] {
            let e = match op {
                "close" => a.close_story(true).unwrap(),
                "map" => a.to_map().unwrap(),
                _ => a.to_roost().unwrap(),
            };
            actual.push(flow_events(&e));
        }
        assert_eq!(json!(actual), j["steps"]);
        compare(a.profile(), &j["me"]);
        assert_eq!(a.state(), j["state"].as_str().unwrap());
    }
}
fn run_js(script: &str) -> Vec<u8> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let wire: String = serde_json::to_string(script)
        .unwrap()
        .chars()
        .map(|c| {
            if c.is_ascii() {
                c.to_string()
            } else {
                c.encode_utf16(&mut [0; 2])
                    .iter()
                    .map(|u| format!(r"\u{u:04x}"))
                    .collect()
            }
        })
        .collect();
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
        .write_all(format!("{wire}\n").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}
fn compare(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert!((a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8)
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                compare(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (k, v) in a {
                compare(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn pending_wrong_answer_after_navigation_mutates_active_profile() {
    let mut a = start(json!({"seenIntro":true,"eggs":1,"eggAwake":true}));
    a.wake_egg(clock(0)).unwrap();
    a.tick(0., Input::default(), view(), clock(1000)).unwrap();
    a.times_key("999");
    a.answer_times(clock(1000)).unwrap();
    // Source setTimeout is not canceled by navigation; its captured wrong answer
    // still freezes the current session and puts its egg back to sleep.
    a.to_map().unwrap();
    a.choose_mission("m1", true).unwrap();
    assert_eq!(a.state(), "play");
    a.tick(0.01, Input::default(), view(), clock(2300)).unwrap();
    assert_eq!(a.state(), "frozen");
    assert_eq!(a.profile()["eggAwake"], false);
    a.tick(0., Input::default(), view(), clock(62300)).unwrap();
    a.thaw(clock(62300)).unwrap();
    assert_eq!(a.state(), "roost");
    assert_eq!(a.profile()["eggAwake"], false);
}

#[test]
fn changing_look_keeps_import_identity_progress_and_adventure_random_stream() {
    let mut a = start(json!({"rid":"r-original","earned":25,"seenIntro":true}));
    let mut b = start(json!({"rid":"r-original","earned":25,"seenIntro":true}));
    let original = a.initial_profile().clone();
    a.to_roost().unwrap();
    b.to_roost().unwrap();
    let events = a
        .change_look(
            &json!({"name":"Skye","dname":"Comet","path":"storm","dkind":"frost"}),
            500,
        )
        .unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Dirty)));
    assert_eq!(a.initial_profile(), &original);
    assert_eq!(a.profile()["rid"], "r-original");
    assert_eq!(a.profile()["earned"].as_f64(), Some(25.));
    assert_eq!(a.profile()["party"][0]["id"], "bond");
    assert_eq!(a.profile()["party"][0]["name"], "Comet");
    for adventure in [&mut a, &mut b] {
        adventure.to_map().unwrap();
        adventure.choose_mission("m1", false).unwrap();
        if adventure.state() == "story" {
            adventure.close_story(true).unwrap();
        }
    }
    assert_eq!(
        a.game.as_ref().unwrap().rng.state,
        b.game.as_ref().unwrap().rng.state
    );
}

#[test]
fn shared_flight_owns_one_game_and_preserves_profile_on_pause_and_retire() {
    use dragon_game::authority::Authority;
    let mut a = start(json!({"seenIntro":true,"briefed":["m1"],"rid":"local-rider"}));
    a.choose_mission("m1", false).unwrap();
    let original = a.profile().clone();
    let authority = Authority {
        guest: false,
        host_peer: None,
        host_rider: 2,
        epoch: 1,
    };
    assert!(
        a.share_flight(String::new(), 2, "run1".into(), authority.clone())
            .is_err()
    );
    assert!(a.game.is_some());
    a.share_flight("fixture-peer".into(), 2, "run1".into(), authority)
        .unwrap();
    assert!(a.game.is_none());
    assert!(a.game().is_some());
    a.tick(0.02, Input::default(), view(), clock(100)).unwrap();
    let position = (a.game().unwrap().player.x, a.game().unwrap().player.y);
    a.pause();
    a.tick(
        0.02,
        Input {
            right: true,
            ..Default::default()
        },
        view(),
        clock(120),
    )
    .unwrap();
    assert_eq!(
        (a.game().unwrap().player.x, a.game().unwrap().player.y),
        position
    );
    assert!(!a.drain_network_outbound().is_empty());
    a.to_roost().unwrap();
    assert!(a.game().is_none());
    assert!(a.shared.is_none());
    assert_eq!(a.profile()["rid"], original["rid"]);
    assert_eq!(a.profile()["party"], original["party"]);
}
