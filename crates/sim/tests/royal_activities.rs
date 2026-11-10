use bevy_math::Vec2;
use crabsim::{
    Controls, PlayerId, Replay, Sim, Skill,
    level::royal as map,
    play,
    royal::{self, Action},
};
const ID: PlayerId = PlayerId(0);
fn at(pos: Vec2) -> Sim {
    let mut sim = Sim::with_one_crab(pos);
    sim.crab_mut(ID).unwrap().mover.grounded = true;
    sim
}
fn book() -> Sim {
    at(Vec2::new(map::BOOK.x, map::FLOOR_Y))
}
fn walk(sim: &mut Sim, x: f32) {
    for _ in 0..120 * 40 {
        let p = sim.crab(ID).unwrap().mover.pos;
        if (p.x - x).abs() < 9.0 {
            sim.set_controls(ID, Controls::default());
            return;
        }
        sim.set_controls(
            ID,
            Controls {
                walk: (x - p.x).signum(),
                ..Controls::default()
            },
        );
        sim.tick();
    }
    panic!("walk to {x}: {:?}", sim.crab(ID).unwrap().mover.pos);
}
fn climb(sim: &mut Sim, y: f32) {
    for tick in 0..120 * 40 {
        if sim.crab(ID).unwrap().mover.pos.y >= y {
            return;
        }
        sim.set_controls(
            ID,
            Controls {
                jump_held: true,
                jump_pressed: tick == 0 || tick == 35,
                ..Controls::default()
            },
        );
        sim.tick();
    }
    panic!("climb to {y}: {:?}", sim.crab(ID).unwrap().mover.pos);
}

#[test]
fn actual_upload_is_once_and_does_not_change_model_or_physical_power() {
    let mut sim = book();
    let before = sim.crab(ID).unwrap().mover.claw_strength(&sim.tuning);
    assert!(!sim.royal_action(
        ID,
        Action::Upload {
            rows: 2,
            columns: 3
        }
    ));
    assert!(sim.royal_action(
        ID,
        Action::Upload {
            rows: 30,
            columns: 4
        }
    ));
    assert!(!sim.royal_action(
        ID,
        Action::Upload {
            rows: 30,
            columns: 4
        }
    ));
    let m = &sim.crab(ID).unwrap().mover;
    assert_eq!(m.data_xp, 40.0);
    assert_eq!(m.level(Skill::DataDriven, &sim.tuning), 2);
    assert_eq!(m.claw_strength(&sim.tuning), before);
    assert!(!m.feat_ready(Skill::DataDriven, &sim.tuning));
    assert!(!sim.moult(ID, Skill::DataDriven));
    sim.crab_mut(ID).unwrap().remote = true;
    assert!(!sim.royal_action(ID, Action::BeginResearch(1)));
    let mut sea = at(crabsim::SPAWN);
    assert!(!sea.royal_action(
        ID,
        Action::Upload {
            rows: 30,
            columns: 4
        }
    ));
}
#[test]
fn only_completed_real_work_receipts_earn_xp_and_replays_cannot_duplicate_it() {
    let mut sim = book();
    assert!(sim.royal_action(ID, Action::BeginResearch(1)));
    assert!(sim.royal_action(ID, Action::CancelResearch(1)));
    assert!(!sim.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 1,
            judged: 16
        }
    ));
    assert_eq!(sim.crab(ID).unwrap().mover.data_xp, 0.0);
    assert!(sim.royal_action(ID, Action::BeginResearch(2)));
    assert!(!sim.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 2,
            judged: 0
        }
    ));
    assert!(sim.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 2,
            judged: 16
        }
    ));
    assert!(!sim.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 2,
            judged: 16
        }
    ));
    assert_eq!(sim.crab(ID).unwrap().mover.data_xp, 34.0);
    assert_eq!(sim.crab(ID).unwrap().royal.tournaments, 1);
    let saved = sim.progress(ID).unwrap();
    let text = serde_json::to_string(&saved).unwrap();
    let mut restored = book();
    restored.restore_progress(ID, &serde_json::from_str(&text).unwrap());
    assert_eq!(
        restored.crab(ID).unwrap().royal,
        sim.crab(ID).unwrap().royal
    );
    assert_eq!(restored.crab(ID).unwrap().mover.data_xp, 34.0);
    assert!(!restored.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 2,
            judged: 16
        }
    ));
}
#[test]
fn asynchronous_work_can_finish_after_its_crab_dies_without_wedging_the_next_ticket() {
    let mut sim = book();
    assert!(sim.royal_action(ID, Action::BeginResearch(1)));
    sim.crab_mut(ID).unwrap().fighter.dead = Some(1);
    assert!(sim.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 1,
            judged: 8
        }
    ));
    assert!(sim.crab(ID).unwrap().research_ticket.is_none());
    assert_eq!(sim.crab(ID).unwrap().mover.data_xp, 22.0);
}
#[test]
fn canonical_cell_actions_credit_the_named_crab_without_sharing_its_workbook() {
    use crabsim::replay::{CellSessionAction, CellSessionTick};
    let mut sim = book();
    sim.enable_underwater_cells();
    let guest = PlayerId(7);
    sim.add_crab(guest, Vec2::new(map::BOOK.x + 30.0, map::FLOOR_Y));
    sim.crab_mut(guest).unwrap().mover.grounded = true;
    let mut tick = CellSessionTick::default();
    tick.actions[0] = Some((
        guest,
        CellSessionAction::Royal(Action::Upload {
            rows: 12,
            columns: 3,
        }),
    ));
    assert!(sim.tick_cell_session(ID, guest, &tick));
    assert_eq!(sim.crab(guest).unwrap().mover.data_xp, 40.0);
    assert_eq!(sim.crab(ID).unwrap().mover.data_xp, 0.0);
    tick.actions[0] = Some((
        PlayerId(99),
        CellSessionAction::Royal(Action::Upload {
            rows: 12,
            columns: 3,
        }),
    ));
    let before = sim.state_hash();
    assert!(!sim.tick_cell_session(ID, guest, &tick));
    assert_eq!(before, sim.state_hash());
}
#[test]
fn old_character_progress_keeps_its_skills_and_starts_a_fresh_data_career() {
    let mut source = book();
    source.crab_mut(ID).unwrap().mover.climb_xp = 250.0;
    let mut json = serde_json::to_value(source.progress(ID).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("data_xp");
    json.as_object_mut().unwrap().remove("royal");
    let progress = serde_json::from_value(json).unwrap();
    let mut loaded = book();
    loaded.restore_progress(ID, &progress);
    let c = loaded.crab(ID).unwrap();
    assert_eq!(c.mover.climb_xp, 250.0);
    assert_eq!(c.mover.data_xp, 0.0);
    assert_eq!(c.royal, royal::Progress::default());
}
#[test]
fn observatory_decisions_require_the_correct_answer_and_pay_once() {
    let mut sim = at(map::OBSERVATORY);
    assert!(!sim.royal_action(
        ID,
        Action::Answer {
            puzzle: 0,
            choice: 1
        }
    ));
    assert!(!sim.royal_action(
        ID,
        Action::Answer {
            puzzle: 255,
            choice: 0
        }
    ));
    for (i, p) in royal::PUZZLES.iter().enumerate() {
        let a = Action::Answer {
            puzzle: i as u8,
            choice: p.answer,
        };
        assert!(sim.royal_action(ID, a));
        assert!(!sim.royal_action(ID, a));
    }
    let c = sim.crab(ID).unwrap();
    assert_eq!(c.royal.puzzles, 7);
    assert_eq!(c.mover.data_xp, 60.0);
    assert_eq!(c.royal.tokens, 6);
}
#[test]
fn index_return_and_ribbon_purchase_are_durable_personal_quests() {
    let mut sim = at(map::ARCHIVIST);
    assert!(sim.royal_action(ID, Action::TalkArchivist));
    for page in map::INDEX_PAGES {
        sim.crab_mut(ID).unwrap().mover.pos = page - Vec2::Y * 55.0;
        sim.tick();
    }
    assert_eq!(sim.crab(ID).unwrap().royal.pages, 7);
    let c = sim.crab_mut(ID).unwrap();
    c.mover.pos = map::ARCHIVIST;
    c.mover.grounded = true;
    assert!(sim.royal_action(ID, Action::TalkArchivist));
    let tokens = sim.crab(ID).unwrap().royal.tokens;
    assert!(sim.royal_action(ID, Action::TalkArchivist));
    assert_eq!(sim.crab(ID).unwrap().royal.tokens, tokens);
    let c = sim.crab_mut(ID).unwrap();
    c.mover.pos = map::WARDROBE;
    c.mover.grounded = true;
    assert!(sim.royal_action(ID, Action::Ribbon(2)));
    assert_eq!(sim.crab(ID).unwrap().royal.tokens, tokens - 3);
    assert!(sim.royal_action(ID, Action::Ribbon(2)));
    assert_eq!(sim.crab(ID).unwrap().royal.tokens, tokens - 3);
    assert_eq!(sim.crab(ID).unwrap().royal.wearing, Some(2));
}
#[test]
fn three_bell_trial_is_reachable_in_order_with_ordinary_controls() {
    for cellular in [false, true] {
        let mut sim = at(map::BELL_KEEPER);
        if cellular {
            sim.enable_underwater_cells();
        }
        assert!(sim.royal_action(ID, Action::StartBells));
        walk(&mut sim, 3000.0);
        climb(&mut sim, map::GALLERY_Y + 100.0);
        assert_eq!(sim.crab(ID).unwrap().bell_run.unwrap().next, 1);
        walk(&mut sim, 1440.0);
        assert_eq!(sim.crab(ID).unwrap().bell_run.unwrap().next, 2);
        walk(&mut sim, 1115.0);
        climb(&mut sim, map::ROOF_Y + 100.0);
        walk(&mut sim, 600.0);
        let c = sim.crab(ID).unwrap();
        assert!(c.bell_run.is_none());
        assert!(
            c.royal
                .best_bell_ticks
                .is_some_and(|t| t < royal::TRIAL_LIMIT)
        );
        assert!(c.mover.pos.y >= map::ROOF_Y - 1.0);
        assert!(c.royal.rooms.count_ones() >= 4);
    }
}
#[test]
fn castle_receipts_roundtrip_through_the_real_replay_format() {
    let mut live = book();
    live.start_recording();
    assert!(live.royal_action(
        ID,
        Action::Upload {
            rows: 12,
            columns: 3
        }
    ));
    assert!(live.royal_action(ID, Action::BeginResearch(1)));
    live.tick();
    assert!(live.royal_action(
        ID,
        Action::CompleteResearch {
            ticket: 1,
            judged: 16
        }
    ));
    for _ in 0..12 {
        live.tick();
    }
    let replay = Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    assert_eq!(replay.royal_actions.len(), 3);
    let mut again = book();
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(live.state_hash(), again.state_hash());
}
#[test]
fn both_castle_shafts_fit_a_fully_moulted_crab_on_either_side() {
    let level = crabsim::Level::test_level();
    let t = crabsim::Tuning::default();
    for x in [1115.0, 3000.0] {
        for side in [-1.0, 1.0] {
            let mut m = crabsim::Mover::new(Vec2::new(x, map::FLOOR_Y + 130.0));
            m.moults = 99;
            m.climbing = true;
            m.grounded = false;
            m.climb_side = side;
            for _ in 0..120 * 60 {
                crabsim::physics::step(
                    &mut m,
                    &Controls {
                        jump_held: true,
                        ..Controls::default()
                    },
                    &t,
                    &level,
                    crabsim::STEP,
                );
                if m.pos.y >= map::ROOF_Y + 100.0 {
                    break;
                }
            }
            assert!(
                m.pos.y >= map::ROOF_Y + 100.0,
                "largest body stopped in shaft {x}, side {side}: {:?}",
                m.pos
            );
        }
    }
}
#[test]
fn the_throne_stairs_can_be_crossed_in_both_directions_after_expansion() {
    for moults in [0, 99] {
        let mut sim = at(map::ARCHIVIST);
        sim.crab_mut(ID).unwrap().mover.moults = moults;
        walk(&mut sim, map::BOOK.x);
        assert!((sim.crab(ID).unwrap().mover.pos.y - map::FLOOR_Y).abs() < 1.0);
        walk(&mut sim, map::ARCHIVIST.x);
    }
}
