//! External API regression: incomplete terrain cannot be extracted as a Sim.
use crabsim::{
    Level, PlayerId, Sim, Tuning,
    underwater::{
        World,
        frozen_chunks::{FrozenChunks, GuardedBuilder, GuardedError},
    },
};

#[test]
fn incomplete_terrain_cannot_be_finished_or_installed() {
    let level = Level::test_level();
    let mut world = World::from_level_settled(&level);
    assert!(world.follow_player(&level, 4400.0, 0.0));
    let frozen = FrozenChunks::capture(&world, 64 * 1024 * 1024).unwrap();
    let ids: Vec<_> = frozen.sleeping_ids().collect();
    assert!(!ids.is_empty());
    let mut sim = Sim::new(Tuning::default(), level);
    assert!(sim.restore_cellular_terrain(world, 17));
    let mut pos = crabsim::SPAWN;
    pos.x = 4400.0;
    sim.add_crab(PlayerId(0), pos);
    sim.level.cells = None;
    let mut builder =
        GuardedBuilder::new(sim, &frozen.metadata().unwrap(), &ids, 2_000_000).unwrap();
    let (cols, rows) = frozen.active_dimensions();
    for y in 0..rows {
        for x in 0..cols {
            builder
                .active(x as u16, y as u16, &frozen.active(x, y).unwrap())
                .unwrap();
        }
    }
    let mut guarded = builder.install().unwrap();
    assert!(matches!(guarded.finish(), Err(GuardedError::Incomplete)));
    assert_eq!(guarded.tick(), Some(17));
    assert_eq!(guarded.missing_count(), ids.len());
}
