#[test]
fn adding_a_non_terrain_tuning_field_keeps_the_fingerprint() {
    let paths: Vec<_> = env!("CRAB_BAKED_PHYSICS_SOURCES").split(';').collect();
    for excluded in [
        "src/world.rs",
        "src/tuning.rs",
        "src/physics.rs",
        "src/level/circulation.rs",
        "src/underwater/settled_seed.rs",
    ] {
        assert!(
            !paths.contains(&excluded),
            "{excluded} must not force a rebake"
        );
    }
    for included in [
        "src/underwater.rs",
        "src/underwater/seed_bake.rs",
        "src/physics/consts.rs",
        "src/level/coral_rise.rs",
    ] {
        assert!(
            paths.contains(&included),
            "{included} must invalidate the seed"
        );
    }
}
