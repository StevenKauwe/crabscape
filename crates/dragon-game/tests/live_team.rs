use dragon_game::{
    adventure::{Adventure, Clock},
    flight::Viewport,
    game::Game,
    live_team, profile, progression,
};
use serde_json::{Value, json};
fn fixture(rider: usize) -> Value {
    let key = ["alexa", "tavian", "rider3", "rider4", "rider5", "rider6"][rider];
    profile::normalize_profile(&json!({"rid":"privateSaveId","xpv":2,"seenIntro":true,"briefed":["m1"],"earned":100,"spent":10,"hatched":4,"craft":{"armor":2,"saddles":true,"tonic":1},"cleared":["m3"],"built":["lanterns"],"build":{"name":"Synthetic","dname":"Ember","dkind":"flame","path":"forge"},"party":[{"id":"bond","bonded":true,"xp":2000},{"id":"privateDragonId","name":"Wing","kind":"spark","xp":600,"color":"#ff1234","bonded":false}],"wild":[{"name":"Wild","kind":"frost","color":"#123456","realm":"frost"}]}),key,rider,String::new).unwrap()
}
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
#[test]
fn all_six_slots_keep_meaningful_progression_without_sharing_save_ids() {
    for rider in 0..6 {
        let p = fixture(rider);
        let wire = live_team::project(&p);
        let text = wire.to_string();
        assert!(!text.contains("private"));
        assert!(wire.get("rid").is_none());
        let received = live_team::admit(&wire, rider).unwrap();
        for key in [
            "earned", "spent", "hatched", "craft", "cleared", "built", "build",
        ] {
            assert_eq!(received[key], p[key], "{key}")
        }
        assert_eq!(received["party"].as_array().unwrap().len(), 2);
        assert_eq!(received["party"][1]["name"], "Wing");
        assert_eq!(received["party"][1]["xp"], 600.);
        assert_eq!(received["wild"][0]["from"], rider);
    }
}
#[test]
fn malformed_projection_never_grants_progression() {
    let good = live_team::project(&fixture(1));
    for (key, value) in [
        ("earned", json!(-1)),
        ("earned", json!(1e12)),
        ("craft", json!({})),
        ("party", json!([])),
        ("build", Value::Null),
        ("wild", json!(vec![json!({}); 41])),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(live_team::admit(&bad, 1).is_none(), "{key}")
    }
    let mut bad = good.clone();
    bad["rid"] = json!("secret");
    assert!(live_team::admit(&bad, 1).is_none());
    assert!(live_team::admit(&good, 6).is_none());
}
#[test]
fn room_team_affects_unlocks_hoard_crafts_flight_and_clears_without_merging_saves() {
    let mut local = fixture(0);
    local["earned"] = json!(0);
    local["spent"] = json!(0);
    local["craft"] = profile::new_profile("alexa")["craft"].clone();
    local["cleared"] = json!([]);
    local["wild"] = json!([]);
    let original = local.clone();
    let remote = live_team::admit(&live_team::project(&fixture(1)), 1).unwrap();
    let mut a = Adventure::new(local, vec![], view(), 42, 0).unwrap().0;
    assert_eq!(a.team_summary().hoard, 0.);
    assert!(!a.mission_unlocked("m4"));
    a.set_live_team(vec![remote.clone()]);
    assert_eq!(a.team_summary().hoard, 90.);
    assert_eq!(a.team_summary().craft["armor"], 2.);
    assert!(a.mission_unlocked("m4"));
    assert_eq!(a.team_wild().len(), 1);
    a.to_roost().unwrap();
    while a.state() == "story" {
        a.close_story(true).unwrap();
    }
    a.roost_command(
        progression::Command::Feed("bond".into()),
        Clock {
            now_ms: 100,
            day: 0,
        },
    )
    .unwrap();
    assert_eq!(a.profile()["spent"], 10.);
    assert_eq!(remote["spent"], 10.);
    assert_eq!(a.profile()["rid"], original["rid"]);
    a.choose_mission("m1", true).unwrap();
    assert_eq!(a.game().unwrap().max_hearts(), 7);
    assert_eq!(a.game().unwrap().player.hearts, 7);
    a.clear_live_team();
    assert_eq!(a.game().unwrap().max_hearts(), 5);
    assert!(a.others.is_empty());
    assert!(a.team_wild().is_empty());
    assert!(!a.mission_unlocked("m4"));
    let solo = Game::from_profile("m1", original, view(), 42).unwrap();
    assert_eq!(solo.max_hearts(), 5);
}

#[test]
fn all_source_build_projects_use_catalog_cost_unlock_and_local_accounting() {
    for project in dragon_game::content::catalog()["projects"]
        .as_array()
        .unwrap()
    {
        let mut local = fixture(0);
        local["built"] = json!([]);
        local["cleared"] = json!([]);
        local["earned"] = json!(10000);
        local["spent"] = json!(0);
        let id = project["id"].as_str().unwrap().to_owned();
        assert_eq!(
            progression::apply(
                &mut local,
                &[],
                true,
                progression::Command::Build(id.clone()),
                1.
            ),
            Err(progression::Error::LockedProject)
        );
        let peer = json!({"cleared":[project["at"]]});
        let events = progression::apply(
            &mut local,
            std::slice::from_ref(&peer),
            false,
            progression::Command::Build(id.clone()),
            2.,
        )
        .unwrap();
        assert_eq!(local["spent"].as_f64(), project["cost"].as_f64());
        assert_eq!(local["built"], json!([id]));
        assert!(events.iter().any(|e|matches!(e,progression::Event::Built{thanks,..} if thanks==project["thanks"].as_str().unwrap())));
        assert_eq!(
            progression::apply(
                &mut local,
                &[peer],
                false,
                progression::Command::Build(id),
                3.
            ),
            Err(progression::Error::Complete)
        );
    }
}
